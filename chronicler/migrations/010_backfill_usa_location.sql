-- Backfill all existing results with 'iah' (Houston) as the default checker location
-- This sets the location for all historical data before multi-region support was added
-- Using IATA airport codes for location identifiers

ALTER TABLE hosh.results UPDATE checker_location = 'iah' WHERE checker_location = '';
