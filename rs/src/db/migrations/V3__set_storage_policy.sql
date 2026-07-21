-- V3: Set storage policy on existing tables for automatic partition expiry
-- Applies DROP LOCAL 1H as default for all market data tables.
-- The default can be overridden at runtime via CLI --retention-window.
-- For new deployments this is already set in V1/V2 via the CREATE TABLE clause.

ALTER TABLE lob_levels SET STORAGE POLICY (DROP LOCAL 1H);
ALTER TABLE trades SET STORAGE POLICY (DROP LOCAL 1H);
ALTER TABLE orderbook_snapshots SET STORAGE POLICY (DROP LOCAL 1H);
