//! The sorting plan (spec §11, Plan): for every indexed file, where it goes,
//! what happens to it and why. Deterministic for the same index, settings and
//! destination contents. Builds the plan only; nothing on disk is changed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::domain::note::Note;
use crate::domain::operation::{ConflictPolicy, OperationMode};
use crate::domain::sort_template::{Fill, Template, sanitize_file_name};
use crate::services::companions::CompanionRole;
use crate::settings::AppSettings;

/// Paths longer than this may trouble older Windows programs (Explorer).
const LONG_PATH_CHARS: usize = 250;

#[derive(Debug, Clone)]
pub struct CompanionInput {
    pub id: i64,
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub mtime: i64,
    pub role: CompanionRole,
}

/// One indexed main file and what the planner needs to know about it.
#[derive(Debug, Clone)]
pub struct PlanInput {
    pub id: i64,
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub mtime: i64,
    pub sha256: Option<String>,
    pub captured: Option<NaiveDateTime>,
    pub place: Option<String>,
    pub companions: Vec<CompanionInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Will be copied or moved.
    Ready,
    /// The same name is taken by a different file; with the suffix policy
    /// the file gets a new name and is `Ready` instead.
    Conflict,
    /// An identical file (same SHA-256) is already at the destination.
    AlreadyArchived,
    /// The file already sits exactly where the plan would put it.
    AlreadyInPlace,
    /// Another file in this plan has the same content; only one is sorted.
    Duplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryRole {
    Main,
    Sidecar,
    LiveVideo,
}

#[derive(Debug, Clone)]
pub struct PlanEntry {
    pub media_id: i64,
    pub role: EntryRole,
    /// Index of the main entry this companion belongs to.
    pub parent: Option<usize>,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub size: u64,
    pub mtime: i64,
    pub sha256: Option<String>,
    pub captured: Option<NaiveDateTime>,
    pub place: Option<String>,
    pub status: PlanStatus,
    /// Whether the file takes part when the plan is run. Only `Ready`
    /// entries start included; the user may change this per entry.
    pub included: bool,
    pub notes: Vec<Note>,
}

/// What is already in the destination folder.
pub trait DestinationProbe {
    /// Size of an existing file at `path`, `None` when nothing is there.
    fn size(&self, path: &Path) -> Option<u64>;
    fn sha256(&self, path: &Path) -> Option<String>;
}

/// Real file system.
pub struct FsProbe;

impl DestinationProbe for FsProbe {
    fn size(&self, path: &Path) -> Option<u64> {
        std::fs::symlink_metadata(path).ok().map(|m| m.len())
    }

    fn sha256(&self, path: &Path) -> Option<String> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let mut file = std::fs::File::open(path).ok()?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0_u8; 1 << 20];
        loop {
            let n = file.read(&mut buf).ok()?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Some(hex::encode(hasher.finalize()))
    }
}

/// Case-insensitive key: Windows and macOS treat `A.jpg` and `a.jpg` as the
/// same file, so two plan entries must never differ only by case.
fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// `IMG_1.HEIC`, 2 → `IMG_1 (2).HEIC`
fn with_suffix(name: &str, n: u32) -> String {
    let (stem, ext) = split_name(name);
    format!("{stem} ({n}){ext}")
}

/// A companion's new name follows its main file: if `IMG_1.HEIC` became
/// `IMG_1 (2).HEIC`, then `IMG_1.AAE` becomes `IMG_1 (2).AAE` and
/// `IMG_1.HEIC.xmp` becomes `IMG_1 (2).HEIC.xmp`.
fn companion_name(main_original: &str, main_final: &str, companion: &str) -> String {
    let (orig_stem, _) = split_name(main_original);
    let (final_stem, _) = split_name(main_final);
    if companion.len() >= orig_stem.len()
        && companion[..orig_stem.len()].eq_ignore_ascii_case(orig_stem)
    {
        format!("{final_stem}{}", &companion[orig_stem.len()..])
    } else {
        companion.to_owned()
    }
}

struct Names<'a> {
    taken: HashSet<String>,
    probe: &'a dyn DestinationProbe,
}

enum Placement {
    Free(PathBuf),
    /// Identical content already there.
    Archived(PathBuf),
}

impl Names<'_> {
    /// First free name in `dir` for `name`, trying ` (1)`, ` (2)`... An
    /// existing file with the same content counts as already archived.
    fn place(&self, dir: &Path, name: &str, size: u64, sha: Option<&str>) -> (Placement, bool) {
        let mut n = 0;
        loop {
            let candidate = if n == 0 {
                name.to_owned()
            } else {
                with_suffix(name, n)
            };
            let path = dir.join(&candidate);
            let in_plan = self.taken.contains(&key(&path));
            if !in_plan {
                match self.probe.size(&path) {
                    None => return (Placement::Free(path), n > 0),
                    Some(existing)
                        if existing == size
                            && sha.is_some()
                            && self.probe.sha256(&path).as_deref() == sha =>
                    {
                        return (Placement::Archived(path), n > 0);
                    }
                    Some(_) => {}
                }
            }
            n += 1;
        }
    }
}

/// Builds the plan. Main files are processed oldest first (then by path), so
/// the same index always gives the same names.
pub fn build(
    inputs: &[PlanInput],
    root: &Path,
    template: &Template,
    settings: &AppSettings,
    mode: OperationMode,
    probe: &dyn DestinationProbe,
) -> Vec<PlanEntry> {
    let _ = mode; // the plan is the same for copy and move; Apply differs
    let mut order: Vec<&PlanInput> = inputs.iter().collect();
    order.sort_by(|a, b| {
        (a.captured.is_none(), a.captured, &a.path).cmp(&(
            b.captured.is_none(),
            b.captured,
            &b.path,
        ))
    });

    let mut names = Names {
        taken: HashSet::new(),
        probe,
    };
    let mut first_with_sha: HashMap<&str, &Path> = HashMap::new();
    let mut entries: Vec<PlanEntry> = Vec::new();

    for input in order {
        let folders = template.folders(&Fill {
            date: input.captured,
            place: input.place.as_deref(),
            month_format: settings.month_format,
            language: settings.language(),
            unknown_date: settings.unknown_date_folder(),
            unknown_place: settings.unknown_place_folder(),
        });
        let dir = folders.iter().fold(root.to_path_buf(), |p, f| p.join(f));
        let file_name = sanitize_file_name(&input.file_name);
        let mut notes = Vec::new();
        if input.captured.is_none() {
            notes.push(Note::new("plan_no_date"));
        }
        if input.place.is_none() {
            notes.push(Note::new("plan_no_place"));
        }

        let duplicate_of = input
            .sha256
            .as_deref()
            .and_then(|sha| match first_with_sha.get(sha) {
                Some(first) => Some(first.to_path_buf()),
                None => {
                    first_with_sha.insert(sha, &input.path);
                    None
                }
            });

        let (status, destination) = if let Some(first) = duplicate_of {
            notes.push(Note::new("plan_duplicate").arg("path", first.display()));
            (PlanStatus::Duplicate, dir.join(&file_name))
        } else if key(&input.path) == key(&dir.join(&file_name)) {
            notes.push(Note::new("plan_in_place"));
            (PlanStatus::AlreadyInPlace, input.path.clone())
        } else {
            match names.place(&dir, &file_name, input.size, input.sha256.as_deref()) {
                (Placement::Archived(path), _) => {
                    notes.push(Note::new("plan_archived").arg("path", path.display()));
                    (PlanStatus::AlreadyArchived, path)
                }
                (Placement::Free(path), renamed) => {
                    let status = if renamed && settings.conflict_policy != ConflictPolicy::AddSuffix
                    {
                        PlanStatus::Conflict
                    } else {
                        PlanStatus::Ready
                    };
                    if renamed {
                        notes.push(
                            Note::new(if status == PlanStatus::Conflict {
                                "plan_conflict"
                            } else {
                                "plan_renamed"
                            })
                            .arg(
                                "name",
                                path.file_name()
                                    .map(|n| n.to_string_lossy())
                                    .unwrap_or_default(),
                            ),
                        );
                    }
                    (status, path)
                }
            }
        };
        if status == PlanStatus::Ready || status == PlanStatus::Conflict {
            names.taken.insert(key(&destination));
        }
        if destination.to_string_lossy().chars().count() > LONG_PATH_CHARS {
            notes.push(Note::new("plan_long_path"));
        }
        let included = status == PlanStatus::Ready;
        let main_index = entries.len();
        let final_name = destination
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dest_dir = destination
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.clone());
        entries.push(PlanEntry {
            media_id: input.id,
            role: EntryRole::Main,
            parent: None,
            source: input.path.clone(),
            destination,
            size: input.size,
            mtime: input.mtime,
            sha256: input.sha256.clone(),
            captured: input.captured,
            place: input.place.clone(),
            status,
            included,
            notes,
        });

        for c in &input.companions {
            let name = companion_name(
                &input.file_name,
                &final_name,
                &sanitize_file_name(&c.file_name),
            );
            let destination = dest_dir.join(&name);
            let mut notes = Vec::new();
            let mut status = status;
            if status == PlanStatus::Ready || status == PlanStatus::Conflict {
                if names.taken.contains(&key(&destination)) || probe.size(&destination).is_some() {
                    // Rare: something else already uses the companion's name.
                    notes.push(Note::new("plan_companion_taken").arg("name", &name));
                    status = PlanStatus::Conflict;
                } else {
                    names.taken.insert(key(&destination));
                }
            }
            entries.push(PlanEntry {
                media_id: c.id,
                role: match c.role {
                    CompanionRole::Sidecar => EntryRole::Sidecar,
                    CompanionRole::LiveVideo => EntryRole::LiveVideo,
                },
                parent: Some(main_index),
                source: c.path.clone(),
                destination,
                size: c.size,
                mtime: c.mtime,
                sha256: None,
                captured: input.captured,
                place: input.place.clone(),
                status,
                included: included && status == PlanStatus::Ready,
                notes,
            });
        }
    }
    entries
}

/// Builds the plan from the index and stores it as the current draft.
/// Reads the destination folder (to avoid overwriting) but changes nothing.
pub fn create_plan(
    db: &mut crate::persistence::Db,
    settings: &AppSettings,
    mode: OperationMode,
) -> crate::CoreResult<i64> {
    use crate::CoreError;
    let root = settings
        .destination_root
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or(CoreError::InvalidValue {
            field: "destination",
            reason: "no archive folder chosen".to_owned(),
        })?;
    let template =
        Template::parse(&settings.folder_template).map_err(|e| CoreError::InvalidSetting {
            field: "folder_template",
            reason: format!("{e:?}"),
        })?;
    let inputs = db.plan_inputs()?;
    let entries = build(
        &inputs,
        Path::new(root),
        &template,
        settings,
        mode,
        &FsProbe,
    );
    db.save_plan(
        root,
        mode,
        &settings.folder_template,
        &serde_json::to_string(settings)?,
        &entries,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::location::Language;
    use chrono::NaiveDate;
    use std::cell::RefCell;

    /// Destination contents as path → (size, sha).
    #[derive(Default)]
    struct Fake(RefCell<HashMap<String, (u64, String)>>);

    impl Fake {
        fn add(&self, path: &str, size: u64, sha: &str) {
            self.0
                .borrow_mut()
                .insert(key(Path::new(path)), (size, sha.to_owned()));
        }
    }

    impl DestinationProbe for Fake {
        fn size(&self, path: &Path) -> Option<u64> {
            self.0.borrow().get(&key(path)).map(|(s, _)| *s)
        }
        fn sha256(&self, path: &Path) -> Option<String> {
            self.0.borrow().get(&key(path)).map(|(_, h)| h.clone())
        }
    }

    fn input(
        id: i64,
        path: &str,
        date: Option<(i32, u32, u32)>,
        place: Option<&str>,
        sha: &str,
    ) -> PlanInput {
        PlanInput {
            id,
            path: PathBuf::from(path),
            file_name: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            size: 100,
            mtime: 1,
            sha256: Some(sha.to_owned()),
            captured: date
                .and_then(|(y, m, d)| NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(10, 0, 0)),
            place: place.map(str::to_owned),
            companions: vec![],
        }
    }

    fn settings(lang: Language) -> AppSettings {
        AppSettings {
            folder_language: Some(lang),
            ..AppSettings::default()
        }
    }

    fn plan(inputs: &[PlanInput], s: &AppSettings, probe: &Fake) -> Vec<PlanEntry> {
        let t = Template::parse(&s.folder_template).unwrap();
        build(
            inputs,
            Path::new("/archive"),
            &t,
            s,
            OperationMode::Copy,
            probe,
        )
    }

    fn dest(e: &PlanEntry) -> String {
        e.destination.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn year_month_place_in_both_languages() {
        let inputs = [
            input(
                1,
                "/src/a/IMG_1.HEIC",
                Some((2025, 8, 13)),
                Some("Карпати"),
                "a",
            ),
            input(2, "/src/b/IMG_2.JPG", None, Some("Париж"), "b"),
            input(3, "/src/c/IMG_3.JPG", Some((2025, 12, 24)), None, "c"),
        ];
        let p = plan(&inputs, &settings(Language::Uk), &Fake::default());
        let d: Vec<String> = p.iter().map(dest).collect();
        assert_eq!(
            d,
            [
                "/archive/2025/08 - Серпень/Карпати/IMG_1.HEIC",
                "/archive/2025/12 - Грудень/Без геолокації/IMG_3.JPG",
                "/archive/Невідома дата/Париж/IMG_2.JPG",
            ]
        );
        assert!(
            p.iter()
                .all(|e| e.status == PlanStatus::Ready && e.included)
        );
        let p = plan(&inputs[..1], &settings(Language::En), &Fake::default());
        assert_eq!(dest(&p[0]), "/archive/2025/08 - August/Карпати/IMG_1.HEIC");
        let p = plan(&inputs[2..], &settings(Language::En), &Fake::default());
        assert_eq!(
            dest(&p[0]),
            "/archive/2025/12 - December/No location/IMG_3.JPG"
        );
    }

    #[test]
    fn same_name_in_plan_gets_suffix_and_is_deterministic() {
        let inputs = [
            input(
                2,
                "/src/b/IMG_1.JPG",
                Some((2025, 8, 13)),
                Some("Lviv"),
                "b",
            ),
            input(
                1,
                "/src/a/img_1.jpg",
                Some((2025, 8, 13)),
                Some("Lviv"),
                "a",
            ),
        ];
        let p = plan(&inputs, &settings(Language::En), &Fake::default());
        let d: Vec<String> = p.iter().map(dest).collect();
        // Same date: the path decides the order; case-insensitive collision.
        assert_eq!(
            d,
            [
                "/archive/2025/08 - August/Lviv/img_1.jpg",
                "/archive/2025/08 - August/Lviv/IMG_1 (1).JPG"
            ]
        );
        assert_eq!(
            p[1].notes
                .iter()
                .filter(|n| n.code == "plan_renamed")
                .count(),
            1
        );
        let again = plan(&inputs, &settings(Language::En), &Fake::default());
        assert_eq!(again.iter().map(dest).collect::<Vec<_>>(), d);
    }

    #[test]
    fn existing_files_are_never_overwritten() {
        let probe = Fake::default();
        probe.add("/archive/2025/08 - August/Lviv/IMG_1.JPG", 100, "other");
        let p = plan(
            &[input(
                1,
                "/src/IMG_1.JPG",
                Some((2025, 8, 1)),
                Some("Lviv"),
                "a",
            )],
            &settings(Language::En),
            &probe,
        );
        assert_eq!(dest(&p[0]), "/archive/2025/08 - August/Lviv/IMG_1 (1).JPG");
        assert_eq!(p[0].status, PlanStatus::Ready);

        // Skip / ask policies: the file is not sorted unless the user says so.
        let s = AppSettings {
            conflict_policy: ConflictPolicy::Skip,
            ..settings(Language::En)
        };
        let p = plan(
            &[input(
                1,
                "/src/IMG_1.JPG",
                Some((2025, 8, 1)),
                Some("Lviv"),
                "a",
            )],
            &s,
            &probe,
        );
        assert_eq!((p[0].status, p[0].included), (PlanStatus::Conflict, false));
    }

    #[test]
    fn rerunning_after_sorting_finds_files_already_archived() {
        let probe = Fake::default();
        probe.add("/archive/2025/08 - August/Lviv/IMG_1.JPG", 100, "a");
        let p = plan(
            &[input(
                1,
                "/src/IMG_1.JPG",
                Some((2025, 8, 1)),
                Some("Lviv"),
                "a",
            )],
            &settings(Language::En),
            &probe,
        );
        assert_eq!(
            (p[0].status, p[0].included),
            (PlanStatus::AlreadyArchived, false)
        );
        // Also when an earlier run had to add a suffix.
        let probe = Fake::default();
        probe.add("/archive/2025/08 - August/Lviv/IMG_1.JPG", 100, "other");
        probe.add("/archive/2025/08 - August/Lviv/IMG_1 (1).JPG", 100, "a");
        let p = plan(
            &[input(
                1,
                "/src/IMG_1.JPG",
                Some((2025, 8, 1)),
                Some("Lviv"),
                "a",
            )],
            &settings(Language::En),
            &probe,
        );
        assert_eq!(p[0].status, PlanStatus::AlreadyArchived);
    }

    #[test]
    fn exact_duplicates_are_sorted_once() {
        let inputs = [
            input(
                1,
                "/src/a/IMG_1.JPG",
                Some((2025, 8, 1)),
                Some("Lviv"),
                "same",
            ),
            input(
                2,
                "/src/b/IMG_1 copy.JPG",
                Some((2025, 8, 1)),
                Some("Lviv"),
                "same",
            ),
        ];
        let p = plan(&inputs, &settings(Language::En), &Fake::default());
        assert_eq!((p[0].status, p[0].included), (PlanStatus::Ready, true));
        assert_eq!((p[1].status, p[1].included), (PlanStatus::Duplicate, false));
        assert_eq!(
            p[1].notes
                .iter()
                .find(|n| n.code == "plan_duplicate")
                .unwrap()
                .args["path"],
            "/src/a/IMG_1.JPG"
        );
    }

    #[test]
    fn file_already_in_place_is_left_alone() {
        let p = plan(
            &[input(
                1,
                "/archive/2025/08 - August/Lviv/IMG_1.JPG",
                Some((2025, 8, 1)),
                Some("Lviv"),
                "a",
            )],
            &settings(Language::En),
            &Fake::default(),
        );
        assert_eq!(
            (p[0].status, p[0].included),
            (PlanStatus::AlreadyInPlace, false)
        );
    }

    #[test]
    fn companions_travel_and_share_the_suffix() {
        let probe = Fake::default();
        probe.add("/archive/2025/08 - August/Lviv/IMG_1.HEIC", 5, "other");
        let mut main = input(1, "/src/IMG_1.HEIC", Some((2025, 8, 1)), Some("Lviv"), "a");
        main.companions = vec![
            CompanionInput {
                id: 2,
                path: "/src/IMG_1.MOV".into(),
                file_name: "IMG_1.MOV".into(),
                size: 1,
                mtime: 1,
                role: CompanionRole::LiveVideo,
            },
            CompanionInput {
                id: 3,
                path: "/src/IMG_1.AAE".into(),
                file_name: "IMG_1.AAE".into(),
                size: 1,
                mtime: 1,
                role: CompanionRole::Sidecar,
            },
            CompanionInput {
                id: 4,
                path: "/src/IMG_1.HEIC.xmp".into(),
                file_name: "IMG_1.HEIC.xmp".into(),
                size: 1,
                mtime: 1,
                role: CompanionRole::Sidecar,
            },
        ];
        let p = plan(&[main], &settings(Language::En), &probe);
        let d: Vec<String> = p.iter().map(dest).collect();
        assert_eq!(
            d,
            [
                "/archive/2025/08 - August/Lviv/IMG_1 (1).HEIC",
                "/archive/2025/08 - August/Lviv/IMG_1 (1).MOV",
                "/archive/2025/08 - August/Lviv/IMG_1 (1).AAE",
                "/archive/2025/08 - August/Lviv/IMG_1 (1).HEIC.xmp",
            ]
        );
        assert!(p.iter().skip(1).all(|e| e.parent == Some(0) && e.included));
        assert_eq!(p[1].role, EntryRole::LiveVideo);
    }

    #[test]
    fn unsafe_names_never_leave_the_archive() {
        let p = plan(
            &[input(
                1,
                "/src/x.jpg",
                Some((2025, 8, 1)),
                Some("../../etc"),
                "a",
            )],
            &settings(Language::En),
            &Fake::default(),
        );
        let d = dest(&p[0]);
        assert!(d.starts_with("/archive/2025/08 - August/"), "{d}");
        assert!(!d.contains("/../"), "{d}");
        assert_eq!(
            p[0].destination
                .components()
                .filter(|c| matches!(c, std::path::Component::ParentDir))
                .count(),
            0
        );
    }
}
