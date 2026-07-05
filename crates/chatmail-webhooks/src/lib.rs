// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Operator webhooks — async JSON POSTs on registration and quota-cap events.

mod dispatcher;
mod settings;

pub use dispatcher::{DeliveryStats, WebhookDispatcher};
pub use settings::{save_webhook_config, webhook_config, WebhookConfig, WebhookEvent};