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
