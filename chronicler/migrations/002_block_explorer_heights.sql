-- Add block_explorer_heights table for storing block height data from explorers
CREATE TABLE IF NOT EXISTS hosh.block_explorer_heights (
    checked_at DateTime64(3, 'UTC'),
    explorer String,
    chain String,
    block_height UInt64,
    response_time_ms Float32,
    error String DEFAULT '',
    dry_run Boolean DEFAULT false
) ENGINE = MergeTree()
ORDER BY (checked_at, explorer, chain)
PARTITION BY toYYYYMM(checked_at);

-- Create indices for better query performance
ALTER TABLE hosh.block_explorer_heights ADD INDEX IF NOT EXISTS idx_explorer_chain (explorer, chain) TYPE minmax GRANULARITY 1;
ALTER TABLE hosh.block_explorer_heights ADD INDEX IF NOT EXISTS idx_checked_at (checked_at) TYPE minmax GRANULARITY 1;

-- Create a materialized view for block height differences between explorers
CREATE MATERIALIZED VIEW IF NOT EXISTS hosh.block_height_diffs
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(time_bucket)
ORDER BY (chain, time_bucket)
AS SELECT
    chain,
    toStartOfHour(checked_at) AS time_bucket,
    argMax(block_height, checked_at) AS max_height,
    argMin(block_height, checked_at) AS min_height,
    max(block_height) - min(block_height) AS height_diff,
    count() AS num_explorers
FROM hosh.block_explorer_heights
GROUP BY chain, time_bucket; 