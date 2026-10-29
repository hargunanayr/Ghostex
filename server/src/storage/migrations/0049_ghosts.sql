-- CDXC:Ghosts 2026-10-29 WHY:
-- A Ghost is a persistent agent: a home project (flagged `isGhost` in its launch settings), one
-- chat session in it that is also a coordinator row (so threads, reports and the Threads panel are
-- the Orchestrator's), and this side row with what only a Ghost has. Like `coordinators` it has no
-- foreign key, so pruning a session or project row never takes the Ghost with it.
-- `projectsJson` is the ordered list of projectIds the Ghost may open threads in. Soul and memory
-- are files in the Ghost's home folder, not columns, so the agent reads them as context files.
-- `automations.ghostId` marks an automation as one of that Ghost's habits.
CREATE TABLE IF NOT EXISTS ghosts (
  ghostId TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  mascot TEXT NOT NULL DEFAULT '',
  color TEXT NOT NULL DEFAULT '',
  homeProjectId TEXT NOT NULL,
  chatSessionId TEXT NOT NULL DEFAULT '',
  projectsJson TEXT NOT NULL DEFAULT '[]',
  showThreadsInSidebar INTEGER NOT NULL DEFAULT 1 CHECK (showThreadsInSidebar IN (0, 1)),
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ghosts_home ON ghosts(homeProjectId, chatSessionId);

ALTER TABLE automations ADD COLUMN ghostId TEXT;

PRAGMA user_version = 49;
