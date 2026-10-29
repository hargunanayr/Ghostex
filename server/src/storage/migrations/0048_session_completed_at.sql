-- CDXC:SessionBoard 2026-10-28 WHY:
-- The Board's Completed column is a fact the user sets, so it is stored on the session row and
-- every client reads the same column. The legacy settle columns are not reused: settle refuses a
-- session waiting on the user, and a Review card is exactly that.
ALTER TABLE sessions ADD COLUMN completedAt TEXT;

PRAGMA user_version = 48;
