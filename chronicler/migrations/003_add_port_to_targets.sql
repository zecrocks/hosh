-- Add port column to targets table
ALTER TABLE hosh.targets
ADD COLUMN IF NOT EXISTS port UInt16 DEFAULT 0;

-- Update existing targets with default ports based on module
ALTER TABLE hosh.targets
UPDATE port = 50002 WHERE module = 'btc' AND port = 0;

ALTER TABLE hosh.targets
UPDATE port = 443 WHERE module = 'zec' AND port = 0;
