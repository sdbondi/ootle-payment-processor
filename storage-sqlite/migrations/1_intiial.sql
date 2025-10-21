/*
 * Copyright 2025 The Tari Project
 * SPDX-License-Identifier: BSD-3-Clause
 */

CREATE TABLE IF NOT EXISTS job_queue
(
    id              INTEGER PRIMARY KEY,
    uuid            TEXT      NOT NULL UNIQUE,
    task            TEXT      NOT NULL,
    payload         TEXT      NOT NULL DEFAULT '{}',
    status          TEXT      NOT NULL DEFAULT 'pending',
    attempts        INT       NOT NULL DEFAULT 0,
    execute_time_ms bigint    NOT NULL DEFAULT 0,
    scheduled_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    result          TEXT,
    failure_reason  TEXT,
    priority        INT       NOT NULL DEFAULT 0,
    created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_job_queue_status_priority_scheduled_at
    ON job_queue (status, priority, scheduled_at);