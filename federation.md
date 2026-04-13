# Federation Feature Implementation Plan

Implementation of a federation control system for Madmail, including an administrative interface, API support, and core enforcement.

## 1. Plan and Understand

The federation feature allows administrators to implement strict, policy-based traffic rules:
- **Base Policy**: Set the default federation posture to either `ACCEPT` (allow all communication) or `REJECT` (block all communication).
- **Domain Exceptions**: Administrators can define exceptions overriding the base policy:
  - If `ACCEPT`, the rule list acts as a **Blocklist** (denying specific rogue servers).
  - If `REJECT`, the rule list acts as an **Allowlist** (creating a closed, trusted federation ring).
- Enforce these policies universally across inbound HTTP (`/mxdeliv`), inbound standard protocols (SMTP), and outbound delivery.

## 2. Infrastructure & API (Go)

### 2.1. Settings & Policy Persistence
- **Policy Setting Key**: Replace the basic toggle with `KeyFederationPolicy = "__FEDERATION_POLICY__"` in `internal/api/admin/resources/settings.go`. Allowed values are `"ACCEPT"` and `"REJECT"`. Defaults to `"ACCEPT"`.
- **Federation Rules Table (`internal/db/models.go`)**: 
  - Create a new GORM database model `FederationRule`:
    ```go
    type FederationRule struct {
        ID        uint   `gorm:"primaryKey"`
        Domain    string `gorm:"uniqueIndex"`
        CreatedAt int64
    }
    ```
  - **Memory-First Architecture**: Email delivery demands near-zero latency. Therefore, the system **MUST NOT** perform DB `SELECT` queries to evaluate policy rules for each incoming/outgoing message.
  - The policies must be pre-loaded from DB into a thread-safe `map[string]struct{}` guarded by a `sync.RWMutex` during `Init()`.
  - **Synchronous Dual-Writes**: Any modification to the rules (addition/deletion) via Dashboard/CLI must be immediately committed to both the database and the active memory map simultaneously.

### 2.2. Message Statistics
- **Existing Counters**: Leverage `outboundMessages` and `receivedMessages` in `framework/module/msgcounter.go` which already track federated traffic.
- **Persistence**: These counters are already flushed to the database via the `flushMessageCounters` logic in `internal/storage/imapsql/imapsql.go`, ensuring they survive restarts.

### 2.3. Admin API Extensions

To provide a seamless backend for the Admin Web Dashboard, the existing REST API will be expanded to expose federation controls and top-level traffic metrics.

#### 2.3.1. Settings API (`internal/api/admin/resources/settings.go`)
- **State Exposure (`GET /admin/settings`)**: 
  - Update the core `AllSettingsResponse` struct to include a new boolean field:
    ```go
    type AllSettingsResponse struct {
        // ... existing fields ...
        FederationEnabled bool `json:"federation_enabled"`
    }
    ```
  - Modify `AllSettingsHandler` to read the `__FEDERATION_ENABLED__` key from the database and map its string value (`"true"` | `"false"`) to the JSON boolean representation.
- **Toggle Endpoint (`POST /admin/settings/federation`)**:
  - Implement `FederationHandler` utilizing the existing `genericDBToggleHandler` pattern. This guarantees that role-based authentication and database consistency are handled securely.
  - The handler expects a JSON body: `{ "enabled": true/false }` and modifies the persistent key accordingly.

#### 2.3.2. Status API (`internal/api/admin/resources/status.go`)
- **System Health & Counters (`GET /admin/status`)**:
  - While individual server stats are managed by the `FederationTracker` (Section 2.4), the global status endpoint provides top-level metrics.
  - Update `StatusResponse` to explicitly expose global traffic counts. While they are stored internally as `outboundMessages` and `receivedMessages`, they will be serialized cleanly:
    ```go
    type StatusResponse struct {
        // ... existing fields ...
        FederationMessages struct {
            Total       int64 `json:"total"`
            Outbound    int64 `json:"outbound"`
            Received    int64 `json:"received"`
        } `json:"federation_messages"`
    }
    ```
  - `StatusHandler` will aggregate `module.GetOutboundMessages()` and `module.GetReceivedMessages()` to populate these values during dashboard load.

#### 2.3.3. Route Registration
- **`internal/endpoint/chatmail/admin.go`** (or equivalent router definitions):
  - Register the new `/admin/settings/federation` route inside the RPC wrapper.
  - Register the new `/admin/federation/rules` handler mapping to `resources.FederationRulesHandler`.

#### 2.3.4. Federation Rules API
To actively support the Federation Policy Dashboard UI, expose a new handler compatible exclusively with the JSON-RPC envelope standardized in `internal/api/admin/admin.go`. 

- **Handler Pipeline**: The frontend Svelte SPA will push an authenticated JSON payload blindly to `POST /api/admin`. The internal architecture of `admin.go` determines routing based on the embedded `"resource"` key.
- **Implementation**: Map the resource `"resource": "/admin/federation/rules"` to a dedicated `resources.FederationRulesHandler` module.
- **Supported Methods**:
  - **List Active Rules (GET)**
    - Payload: `{ "method": "GET", "resource": "/admin/federation/rules" }`
    - Action: Iterates the `FederationTracker` memory pointer and returns the mapped domain list natively as a pure JSON array string without a DB query.
  - **Create Rule (POST)**
    - Payload: `{ "method": "POST", "resource": "/admin/federation/rules", "body": {"domain": "spam.com"} }`
    - Action: Adds `spam.com` structurally to the `FederationRule` MySQL/SQLite persistence table and seamlessly commits it to the hot RAM memory map instantly.
  - **Destroy Rule (DELETE)**
    - Payload: `{ "method": "DELETE", "resource": "/admin/federation/rules", "body": {"domain": "spam.com"} }`
    - Action: Drops the row utilizing standard GORM where statements, then locks the memory map via `sync.RWMutex` and pops the domain reference perfectly transparently.

### 2.4. Advanced Federation Metrics & Tracking (Go)

To provide detailed diagnostic capabilities without overloading the database or causing disk I/O bottlenecks during live email traffic, the system will use a highly concurrent in-memory caching strategy.

#### 2.4.1. The In-Memory Federation Tracker
- **Data Structure**: Create a new Go package (e.g., `internal/federationtracker`) exposing a singleton `FederationTracker`.
- **Concurrency Control**: Use a `sync.RWMutex` to protect the internal data. This allows multiple Admin Web dashboard API requests to read the stats concurrently (`RLock()`) without blocking the core email delivery engine that writes to it (`Lock()`).
- **Schema**:
  ```go
  type ServerStat struct {
      Domain               string
      QueuedMessages       int64
      FailedHTTP           int64
      FailedHTTPS          int64
      FailedSMTP           int64
      SuccessfulDeliveries int64 // Used to calculate mean latency
      TotalLatencyMs       int64 // Accumulative delivery duration
      LastActive           int64 // Unix timestamp of last interaction
  }

  type FederationTracker struct {
      mu    sync.RWMutex
      stats map[string]*ServerStat
  }
  ```
- **Clear-Text Storage**: Unlike `servertracker` which hashes IPs for absolute privacy, `FederationTracker` will store clear-text server domains (e.g., `example.com` or `1.2.3.4`) exclusively for administrative diagnosis of the federation mesh.

#### 2.4.2. Event-Driven Queuing (No Disk Scanning)
- **Problem**: Scanning the `internal/target/queue` disk folder to count pending messages per server would cause severe blocking during API calls.
- **Solution**: The queue management code (`internal/target/queue/queue.go`) will proactively push state changes to the tracker.
  - When a message is scheduled to a domain: `tracker.IncrementQueue("example.com")`
  - When a message is successfully delivered or permanently bounded: `tracker.DecrementQueue("example.com")`

#### 2.4.3. Failure Transport Classification
- **Integration Point**: The actual remote connection mechanisms in `internal/target/remote/remote.go`.
- **Classification**: Every time a delivery is attempted and fails, an event is emitted: `tracker.RecordFailure("example.com", "HTTPS")`.
- **Transport Types**: Specifically differentiate between `HTTP` (unencrypted proxy routing), `HTTPS` (direct web delivery), and `SMTP` (legacy fallback protocol). This helps administrators identify configuration/firewall issues.

#### 2.4.4. Mean Latency Profiling
- **Calculation Logic**: When an outbound remote delivery formally succeeds, the delivery loop reports the total transaction duration back to the tracker via `tracker.RecordSuccess("example.com", 320)`.
- **Formula Strategy**: The tracker actively avoids expensive mathematical loops by simply adding `320` to `TotalLatencyMs` and incrementing `SuccessfulDeliveries`. The `GET /admin/federation/servers` API endpoint lazily resolves the metric (`TotalLatencyMs / SuccessfulDeliveries`) prior to returning JSON.

#### 2.4.5. Background DB Persistence (The Flusher)
- **Zero-Block Database Writes**: Do not `INSERT`/`UPDATE` the database synchronously during email handling.
- **Flushing Goroutine**: Launch a background worker (`go tracker.StartFlusher(storage.GORMDB)`) on initialization that utilizes a `time.Ticker(30 * time.Second)`.
- **Batch UPSERT**: 
  - Every 30 seconds, the flusher locks the map temporarily, clones the map data, unlocks immediately, and pushes the stats to the underlying SQL table (e.g., `federation_server_stats`). 
  - Uses an `ON CONFLICT (domain) DO UPDATE` (UPSERT) pattern to ensure minimal DB strain. This pattern mirrors the already highly proven `flushMessageCounters()` logic present in `imapsql.go`. 
- **Startup Hydration**: On server start, `Init()` runs a `SELECT *` from `federation_server_stats` to pre-load the RAM map, ensuring seamless survival across server restarts or crashes.

#### 2.4.6. Federation Servers API
- **Endpoint**: Create an Admin API `GET /admin/federation/servers`.
- **Handler Logic**: It directly queries the RAM (via `tracker.GetAll()`) and returns an aggregated JSON array, enabling instantaneous Admin UI rendering without hitting the database.

### 2.5. Command Line Controls (CLI)

To afford system administrators rapid, headless control over federation without requiring dashboard access, the following commands will be integrated directly into the core `internal/cli/ctl` module. These map directly to the internal API state updates, guaranteeing synchronous RAM and database parity.

#### 1. Set Policy
Toggle the global federation posture between open and restricted routing.
*   **Input**: `madmail ctl federation policy accept` (or `reject`)
*   **Action**: Triggers a database write setting `__FEDERATION_POLICY__` to the new state.
*   **Output**:
    ```text
    Success: Global federation policy switched to ACCEPT.
    ```

#### 2. Block Domain
Add a domain to the rules table specifically meant for use when operating under an open policy.
*   **Input**: `madmail ctl federation block spam.com`
*   **Action**: Validates domain formatting automatically, updates the `FederationRule` DB table, and instantly pushes `spam.com` to the memory proxy map.
*   **Output**: 
    ```text
    Success: 'spam.com' added to rules. Currently blocking 1 total domain(s).
    ```

#### 3. Allow Domain
Add a domain to the trusted rules table when operating under restricted routing.
*   **Input**: `madmail ctl federation allow trusted.org`
*   **Action**: Internal mechanics are identical to `block`; contextually intended for REJECT mode.
*   **Output**:
    ```text
    Success: 'trusted.org' added to rules. Currently trusting 5 total domain(s).
    ```

#### 4. Remove Rule
Delete a previously assigned domain exception seamlessly.
*   **Input**: `madmail ctl federation remove spam.com`
*   **Action**: Executes a `DELETE` in GORM by domain string, removes from RAM `sync.RWMutex` map.
*   **Output**:
    ```text
    Success: Removed 'spam.com' from rules. 0 remaining.
    ```

#### 5. Flush Mode
Provides an emergency override to completely clear the entire exception list in one stroke.
*   **Input**: `madmail ctl federation flush`
*   **Action**: Executes a destructive `DELETE FROM federation_rules` in GORM and thoroughly flushes the `map[string]struct{}` back to a zero-length state over the `sync.RWMutex` lock.
*   **Output**:
    ```text
    WARNING: Configuration flushed. 0 custom domains remain in active list.
    ```

#### 6. List Rules
Dump a clean, tabular readout of all active rule configurations locally.
*   **Input**: `madmail ctl federation list`
*   **Action**: Prints the current base policy and iteratively ranges over the memory tracker. (Actively prevents hitting the database for `SELECT` during the CLI invocation).
*   **Output**:
    ```text
    [ FEDERATION STATE ]
    Policy:   ACCEPT

    [ ACTIVE RULES ]
    1. badactor.com      (Added: 2026-04-12)
    2. evildomain.org    (Added: 2026-04-13)
    ---
    Total: 2 exceptions.
    ```

#### 7. Diagnostics
View the live metrics tracked natively in RAM via the `FederationTracker`.
*   **Input**: `madmail ctl federation status`
*   **Action**: Queries the `FederationTracker` map to show outbound diagnostic anomalies.
*   **Output**:
    ```text
    [ TRAFFIC ANOMALIES ]
    - example.com : 12 pending queue / 5 Failed (HTTPS)
    - trusted.org : 0 pending queue  / 0 Failed
    ```

## 3. Core Logic Enforcement (Go)

A globally accessible helper function (e.g., `CheckFederationPolicy(domain string) bool`) will evaluate the target domain against the `__FEDERATION_POLICY__` setting and the `FederationRule` DB table. If `true`, communication proceeds. If `false`, it intercepts and drops the connection.

> [!NOTE]
> **Domain Normalization**: Email formats natively support IP literals in the address block. The `CheckFederationPolicy` helper function must strictly normalize these representations such that routing identifiers like `a@[1.1.1.1]` and `a@1.1.1.1` both safely resolve to the exact same parent domain block: `1.1.1.1`. This ensures flawless, identical enforcement across direct-IP delivery and standard canonical domain federation.

### 3.1. Inbound Federation Enforcement
All incoming federation attempts must be evaluated against the policy using the sender's payload domain.
- **HTTP / Webxdc Enforcement**:
  - **File**: `internal/endpoint/chatmail/chatmail.go`
  - **Location**: Inside `handleReceiveEmail` (the `/mxdeliv` HTTP handler).
  - **Action**: Extract `mailFrom` domain, check the policy, and immediately return a `403 Forbidden` if denied. Note: `receivedMessages` is naturally incremented on successful delivery.
- **SMTP Protocol Enforcement**:
  - **File**: `internal/endpoint/smtp/session.go`
  - **Location**: Processed early in the transaction, typically inside `startDelivery(...)` or the `Mail(from...)` hook.
  - **Action**: Extract the sender domain from the SMTP envelope `FROM:`, evaluate the policy, and reject the transaction with a standard `554 5.7.1 Policy Rejection` SMTP error if denied.

### 3.2. Outbound Federation Enforcement
Outbound delivery must absolutely not waste connection/TCP resources on blocked domains.
- **File**: `internal/target/remote/remote.go`
- **Location**: Intercepted inside the early phases of `Delivery()` loop or `Target.Start()`.
- **Action**: Evaluate the recipient's domain against the federation policy, and instantly abort the delivery returning a permanent failure if the target domain is rejected.

## 4. Admin Web Interface (SvelteKit)

### 4.1. Federation Policy Dashboard
- **Route**: Create `admin-web/src/routes/federation/+page.svelte`.
- **UI Components**:
    - **Header**: "Federation Policy Management".
    - **Policy Radio Switch**: A styled interactive toggle to instantly switch the mesh posture between `ACCEPT` (Open Federation) and `REJECT` (Closed Federation).
    - **Rules Management Table**: An interactive list to add, view, and delete domain exceptions. 
      - When the policy switch is `ACCEPT`, the UI clearly labels this list as **"Blocked Domains"**. 
      - When the policy switch is `REJECT`, the UI dynamically labels this list as **"Trusted Domains"**.
    - **Warning Alert**: A dynamic warning explaining the consequences of the current posture selection.

### 4.2. Navigation
- **Sidebar/Tabs**: Add a "Federation" link to the main navigation in `admin-web/src/routes/+layout.svelte`. Use a relevant icon (e.g., `Globe` or `Network` from Lucide).

### 4.3. Federation Servers Details View
- **Servers Table/Grid**: Add a data table to the Federation tab to list all connected servers.
- **Data Columns**:
    - **Server Domain/IP**: Identity of the remote server.
    - **Mean Latency**: Displays a millisecond metric calculated linearly from `(TotalLatencyMs / SuccessfulDeliveries)`, indicating average time-to-deliver.
    - **Queued Messages**: Live count of pending messages heading to this server.
    - **Failure Diagnostics**: Columns/badges indicating total delivery failures to this server, broken down by transport (`HTTP`, `HTTPS`, `SMTP`).
- **Functionality**: Allow administrators to discover routing issues or offline servers by sorting columns by queue sizes or highest failure rates.

## 5. Polish and Optimize

- **Visuals**: Use CSS transitions for the toggle and counters.
- **Responsiveness**: Ensure the federation tab looks great on mobile and desktop.
- **Feedback**: Provide instant toast notifications when the federation status is changed.

---

> [!IMPORTANT]
> Disabling federation or rejecting a domain will block all incoming mail from external servers and prevent users from sending mail to external domains. Local delivery (user-to-user on the same domain) will remain fully functional and bypass these policies.

## 6. Functional Test Scenarios

To guarantee a bulletproof implementation, the following scenarios must be validated through automated e2e testing or manual verification:

### 6.1. Inbound `ACCEPT` Policy Validations
- **Scenario 1**: Server is set to `ACCEPT`. Incoming `/mxdeliv` request from `random.com` arrives -> **EXPECT: Delivery Success**.
- **Scenario 2**: `badactor.com` is added to the Federation Rules table. Incoming SMTP payload in `session.go` from `badactor.com` arrives -> **EXPECT: Immediate rejection with `554 5.7.1`**.

### 6.2. Inbound `REJECT` Policy Validations
- **Scenario 3**: Server is set to `REJECT`. Incoming `/mxdeliv` request from `random.com` arrives -> **EXPECT: Rejection with `403 Forbidden`**.
- **Scenario 4**: `partner-server.org` is added to the Federation Rules table. Incoming SMTP payload in `session.go` from `partner-server.org` arrives -> **EXPECT: Delivery Success**.

### 6.3. Outbound Policy Validations
- **Scenario 5**: Server is set to `REJECT` policy. User explicitly attempts to send an encrypted Delta Chat message to `someone@random.com`. -> **EXPECT: Permanent Failure**, the message never leaves the `remote.go` delivery process and is bounced.
- **Scenario 6**: `partner-server.org` is in the allowed rules. Outbound message sent to `someone@partner-server.org` -> **EXPECT: Delivery Success**.

### 6.4. Domain Normalization Edge Cases
- **Scenario 7**: Server is `ACCEPT`. Add `1.1.1.1` to the blocked Rules list. Incoming HTTP payload originates from bracketed IP `[1.1.1.1]` -> **EXPECT: 403 Forbidden**, proving the normalizer securely strips brackets before enforcement validation.
- **Scenario 8**: Add `EXAMPLE.COM` (uppercase) to rules. Evaluated payload originates from `example.com` -> **EXPECT: Correct policy enforcement** confirming case-insensitive evaluation routing.

### 6.5. Hybrid Memory/Database Architecture Tests
- **Scenario 9**: Server is `ACCEPT`. Live-add `live.test.local` to the blocklist via API. Instantly send payload from `live.test.local` (without restarting server) -> **EXPECT: Immediate rejection**, validating that changes propagate synchronously to the hot memory map.
- **Scenario 10**: Complete a full service reboot (`systemctl restart`). Send an identical payload from `live.test.local` -> **EXPECT: Immediate rejection**, confirming correct bootstrap hydration from the `FederationRule` DB table.

### 6.6. Local Routing Bypass & Edge Constraints
- **Scenario 11**: Server is `REJECT` (denying all external traffic). User on `chatmail.local` attempts to send an email to another user on `chatmail.local`. -> **EXPECT: Delivery Success**, proving that the internal routing machinery entirely bypasses the federation gatekeeper.
- **Scenario 12**: CLI attempts to add the same blocked domain twice (`madmail ctl federation block example.com` x2) -> **EXPECT: Graceful warning / ignored**, asserting the unique index constraints preventing application crashes.
- **Scenario 13**: Subdomain validation. Block `baddomain.com`. Attempt to receive mail from `mail.baddomain.com`. -> **EXPECT: Rejection**, validating that the policy checker evaluates base domains securely to prevent nested evasion (if base-domain matching is implemented; otherwise define precise exact-match rules).

### 6.7. High Concurrency Safety
- **Scenario 14**: Inject 5,000 asynchronous `/mxdeliv` requests against the HTTP endpoint while a separate background script continuously toggles the main `__FEDERATION_POLICY__` setting from `ACCEPT` to `REJECT` every 10 milliseconds -> **EXPECT: Zero application panics**, explicitly verifying the structural integrity of the `sync.RWMutex` lock mechanisms during massive traffic strain.
