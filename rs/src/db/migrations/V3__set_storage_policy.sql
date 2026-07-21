-- V3: Set TTL on existing tables for automatic partition expiry
-- Applies 1 HOUR TTL as default for all market data tables.
-- The default can be overridden at runtime via CLI --retention-window.
-- For new deployments this is already set in V1/V2 via the CREATE TABLE clause.

ALTER TABLE lob_levels SET TTL 1 HOURS;
ALTER TABLE trades SET TTL 1 HOURS;
