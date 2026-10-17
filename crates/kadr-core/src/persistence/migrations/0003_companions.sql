-- Stage C: how a companion file relates to its main file:
-- 'sidecar' (AAE, XMP, THM) or 'live_video' (the MOV half of a Live Photo).
ALTER TABLE media_item ADD COLUMN companion_role TEXT;
