-- Add migration script here

CREATE TABLE IF NOT EXISTS parcel_history (
    id SERIAL PRIMARY KEY,
    parcel_id TEXT NOT NULL,
    driver_id TEXT NOT NULL,
    latitude FLOAT NOT NULL,
    longitude FLOAT NOT NULL,
    timestamp BIGINT NOT NULL,
    status TEXT NOT NULL

);

ALTER TABLE parcel_history
ADD CONSTRAINT unique_parcel_time UNIQUE (parcel_id, timestamp);
