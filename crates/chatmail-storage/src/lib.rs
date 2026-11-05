// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod blob;
pub mod cas;
pub mod delivery_batch;
pub mod external_store;
pub mod fsync_batch;
pub mod inbox;
pub mod maildir;
pub mod maildir_cache;
pub mod maildir_message;
pub mod purge;
pub mod storage_policy;
pub mod uidlist;

pub use external_store::{ExternalKey, ExternalStore, FsStore};
pub use inbox::{list_inbox, InboxEntry};

pub use blob::{
    commit_mailbox_blob_from_tmp, delete_blob, deliver_local_messages,
    never_delivery_batcher_coordinator_count, read_blob, read_blob_known, read_blob_range_known,
    stream_append_direct_final_no_hash, stream_append_to_tmp, write_blob, write_blob_mailbox,
    write_blob_mailbox_stream, DeliveryOutcome,
};
pub use cas::{hash_bytes, ContentStore};
pub use maildir::{mailbox_exists, MailboxStore, MaildirPaths};
pub use maildir_message::{
    copy_message, expunge_deleted, list_mailbox_messages, move_message, split_maildir_filename,
    store_add_flags, MailboxListing, MaildirFlags, StoredMessage,
};
pub use purge::{
    prune_unread_older, purge_all_mail_blobs, purge_mail_blobs_older, purge_read_messages,
    purge_user_messages,
};
pub use storage_policy::{FsyncMode, StoragePolicy};
