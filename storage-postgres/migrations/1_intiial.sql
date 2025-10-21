/*
 * Copyright 2025 The Tari Project
 * SPDX-License-Identifier: BSD-3-Clause
 */

CREATE TABLE IF NOT EXISTS job_queue
(
    id              UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    task            TEXT        NOT NULL,
    payload         JSONB       NOT NULL DEFAULT '{}',
    status          TEXT        NOT NULL DEFAULT 'pending',
    attempts        INT         NOT NULL DEFAULT 0,
    execute_time_ms bigint      NOT NULL DEFAULT 0,
    scheduled_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    result          JSONB,
    failure_reason  TEXT,
    priority        INT         NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_job_queue_status_priority_scheduled_at
    ON job_queue (status, priority, scheduled_at);