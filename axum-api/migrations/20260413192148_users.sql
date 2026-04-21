-- Add migration script here
CREATE TABLE customer_profiles (
    id UUID PRIMARY KEY,
    default_address_id TEXT DEFAULT NULL
);

CREATE TABLE driver_profiles (
    id UUID PRIMARY KEY,
    is_available BOOLEAN DEFAULT FALSE
);



CREATE TABLE users (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    password TEXT NOT NULL,
    email TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    role TEXT NOT NULL DEFAULT 'customer',
    customer_profile_id UUID REFERENCES customer_profiles(id),
    driver_profile_id UUID REFERENCES driver_profiles(id)
);
