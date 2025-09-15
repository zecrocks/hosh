-- Add community flag to targets table
-- This indicates whether a server is run by a community member

ALTER TABLE hosh.targets 
ADD COLUMN IF NOT EXISTS community Boolean DEFAULT false;

-- Update existing ZEC targets to mark community servers
-- Based on the servers defined in discovery/src/main.rs comments

-- Community servers (those marked with "Community nodes" comment)
ALTER TABLE hosh.targets 
UPDATE community = true 
WHERE module = 'zec' AND (
    hostname = 'zcash.mysideoftheweb.com' OR  -- eZcash
    hostname = 'zaino.stakehold.rs' OR
    hostname = 'lightwalletd.stakehold.rs' OR
    hostname LIKE 'lwd%.zcash-infra.com'  -- Ywallet nodes
);

-- All other ZEC servers remain community = false (official zec.rocks servers)
