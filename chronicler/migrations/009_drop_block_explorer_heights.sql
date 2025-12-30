-- Drop unused block_explorer_heights table and related views
-- This table was created for HTTP block explorer monitoring which has been removed.

DROP VIEW IF EXISTS hosh.block_height_diffs;
DROP TABLE IF EXISTS hosh.block_explorer_heights;
