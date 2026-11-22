# Graph Report - crane  (2026-11-22)

## Corpus Check
- cluster-only mode — file stats not available

## Summary
- 1933 nodes · 4265 edges · 82 communities (74 shown, 8 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 153 edges (avg confidence: 0.84)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Community 0
- Community 1
- Community 2
- Community 3
- Community 4
- Community 5
- Community 6
- Community 7
- Community 8
- Community 9
- Community 10
- Community 11
- Community 12
- Community 13
- Community 14
- Community 15
- Community 16
- Community 17
- Community 18
- Community 19
- Community 20
- Community 21
- Community 22
- Community 23
- Community 24
- Community 25
- Community 26
- Community 27
- Community 28
- Community 29
- Community 30
- Community 31
- Community 32
- Community 33
- Community 34
- Community 35
- Community 36
- Community 37
- Community 38
- Community 39
- Community 40
- Community 41
- Community 42
- Community 43
- Community 44
- Community 45
- Community 46
- Community 47
- Community 48
- Community 49
- Community 50
- Community 51
- Community 52
- Community 53
- Community 54
- Community 55
- Community 56
- Community 57
- Community 58
- Community 59
- Community 60
- Community 61
- Community 62
- Community 63
- Community 64
- Community 65
- Community 66
- Community 67
- Community 68
- Community 69
- Community 70
- Community 71
- Community 72
- Community 73
- Community 74
- Community 75
- Community 76
- Community 78
- Community 81

## God Nodes (most connected - your core abstractions)
1. `App()` - 41 edges
2. `AppState` - 33 edges
3. `call()` - 32 edges
4. `SendError` - 28 edges
5. `Engine` - 27 edges
6. `react` - 24 edges
7. `Pod` - 22 edges
8. `Imported` - 21 edges
9. `Notes` - 21 edges
10. `from_text()` - 19 edges

## Surprising Connections (you probably didn't know these)
- `box_run()` --calls--> `forward()`  [INFERRED]
  src-tauri/src/boxes.rs → crates/crane-box/src/engine.rs
- `send_request()` --calls--> `mask_response()`  [INFERRED]
  src-tauri/src/lib.rs → crates/crane-core/src/send.rs
- `send_request()` --calls--> `mask()`  [INFERRED]
  src-tauri/src/lib.rs → crates/crane-core/src/vars.rs
- `CollectionView` --references--> `Node`  [EXTRACTED]
  src-tauri/src/lib.rs → crates/crane-core/src/collection.rs
- `Boxes` --references--> `RunningStack`  [EXTRACTED]
  src-tauri/src/boxes.rs → crates/crane-box/src/stack.rs

## Import Cycles
- 1-file cycle: `crates/crane-box/src/stack.rs -> crates/crane-box/src/stack.rs`
- 1-file cycle: `crates/crane-box/src/proto.rs -> crates/crane-box/src/proto.rs`
- 2-file cycle: `crates/crane-demo-api/src/lib.rs -> crates/crane-demo-api/src/orders.rs -> crates/crane-demo-api/src/lib.rs`
- 2-file cycle: `crates/crane-core/src/lib.rs -> crates/crane-core/src/send.rs -> crates/crane-core/src/lib.rs`
- 2-file cycle: `crates/crane-core/src/lib.rs -> crates/crane-core/src/url.rs -> crates/crane-core/src/lib.rs`
- 3-file cycle: `crates/crane-core/examples/timing.rs -> crates/crane-core/src/send.rs -> crates/crane-core/src/lib.rs -> crates/crane-core/examples/timing.rs`
- 3-file cycle: `crates/crane-core/src/lib.rs -> crates/crane-core/src/send.rs -> crates/crane-core/src/url.rs -> crates/crane-core/src/lib.rs`
- 4-file cycle: `crates/crane-core/examples/timing.rs -> crates/crane-core/src/send.rs -> crates/crane-core/src/url.rs -> crates/crane-core/src/lib.rs -> crates/crane-core/examples/timing.rs`

## Communities (82 total, 8 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.07
Nodes (47): Collection, CollectionError, creates_folders_and_deletes_request_files(), err(), EXT, io(), keeps_files_with_several_requests(), lists_folders_first_and_only_request_files() (+39 more)

### Community 1 - "Community 1"
Cohesion: 0.06
Nodes (66): @tauri-apps/api, @tauri-apps/api@^2.12.3, asError(), boxLogs(), boxPull(), boxRemove(), boxRemoveImage(), boxRun() (+58 more)

### Community 2 - "Community 2"
Cohesion: 0.06
Nodes (35): Args, Command, Help, Import, Run, Version, DEFAULT_TIMEOUT_SECS, HELP (+27 more)

### Community 3 - "Community 3"
Cohesion: 0.10
Nodes (35): AppState, blocking(), close_collection(), collection_tree(), CollectionView, create_folder(), delete_request(), ENV_FILE (+27 more)

### Community 4 - "Community 4"
Cohesion: 0.07
Nodes (25): add(), Phases, SharedPhases, Timed, timed_builder(), Timed<S>, TimedResolver, TimingLayer (+17 more)

### Community 5 - "Community 5"
Cohesion: 0.10
Nodes (44): Network, NO_NETWORK, RequestSpec, EditEnv, EditVar, editVar(), EnvEditor(), newId() (+36 more)

### Community 6 - "Community 6"
Cohesion: 0.05
Nodes (48): name, runs-on, steps, strategy, GH_TOKEN, GITHUB_TOKEN, TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD (+40 more)

### Community 7 - "Community 7"
Cohesion: 0.07
Nodes (32): @tauri-apps/plugin-updater, @tauri-apps/plugin-updater@^2.14.1, getPolicy(), IN_TAURI, METHODS, OPEN_POLICY, Policy, Report (+24 more)

### Community 8 - "Community 8"
Cohesion: 0.10
Nodes (42): check(), CheckResult, checks_headers_and_body(), checks_json(), checks_status_and_time(), Expect, explains_unknown_checks(), parse_expect() (+34 more)

### Community 9 - "Community 9"
Cohesion: 0.08
Nodes (38): PortMap, runCollection(), sendFromBrowser(), sendRequest(), boxes, checkAll(), closeCollection(), collectionTree (+30 more)

### Community 10 - "Community 10"
Cohesion: 0.11
Nodes (36): append(), clear(), entry(), HIDDEN, hides_secrets_in_error_text(), HistoryEntry, keeps_entries_between_runs_without_secrets(), keeps_only_the_newest() (+28 more)

### Community 11 - "Community 11"
Cohesion: 0.05
Nodes (41): build, cap_drop, cpus, environment, logging, mem_limit, pids_limit, read_only (+33 more)

### Community 12 - "Community 12"
Cohesion: 0.06
Nodes (41): @fontsource/ibm-plex-mono, @fontsource/ibm-plex-sans, jsdom, react-dom, @tauri-apps/cli, @tauri-apps/plugin-process, @testing-library/dom, @types/node (+33 more)

### Community 13 - "Community 13"
Cohesion: 0.06
Nodes (39): concurrency, cancel-in-progress, group, name, runs-on, steps, run, env (+31 more)

### Community 14 - "Community 14"
Cohesion: 0.10
Nodes (32): names_are_simple(), valid_name(), DEFAULT_READY_SECS, down(), explains_mistakes(), FILE, folder(), HTTP_DEFAULT_PORT (+24 more)

### Community 15 - "Community 15"
Cohesion: 0.09
Nodes (20): @testing-library/react, vitest, @testing-library/react@^16.3.3, readRequest(), reset(), resetBoxes(), resetCollection(), resetHistory() (+12 more)

### Community 16 - "Community 16"
Cohesion: 0.06
Nodes (5): CONNECT_TIMEOUT, DEFAULT_PATH, DEVICES, FORWARDED, TICK

### Community 17 - "Community 17"
Cohesion: 0.16
Nodes (24): box_logs(), box_pull(), box_remove(), box_remove_image(), box_run(), box_status(), box_stop(), Boxes (+16 more)

### Community 18 - "Community 18"
Cohesion: 0.13
Nodes (9): an_unknown_ca_is_refused_with_a_hint(), can_skip_the_check_outside_production(), explains_unusable_certificate_files(), get(), goes_through_the_proxy(), Pki, sends_a_client_certificate_when_the_server_wants_one(), trusts_a_ca_from_a_file() (+1 more)

### Community 19 - "Community 19"
Cohesion: 0.15
Nodes (25): makes_readable_file_names(), slug(), add_environments(), apply(), drop_bad_checks(), folder_name(), free_id(), free_label() (+17 more)

### Community 20 - "Community 20"
Cohesion: 0.16
Nodes (20): Shared, create(), delete(), get_one(), list(), LIST_LIMIT, Login, login() (+12 more)

### Community 21 - "Community 21"
Cohesion: 0.14
Nodes (24): CheckResult, Header, check(), Found, millis(), read(), ok(), res (+16 more)

### Community 22 - "Community 22"
Cohesion: 0.22
Nodes (24): apply_auth(), auth_value(), build_url(), FRAME, HEADER_GET, is_collection(), is_environment(), JSON_VAR (+16 more)

### Community 23 - "Community 23"
Cohesion: 0.22
Nodes (25): apply_auth(), Block, blocks(), folder_meta(), folder_seq(), folder_title(), get(), Inherited (+17 more)

### Community 24 - "Community 24"
Cohesion: 0.08
Nodes (27): name, runs-on, steps, run, jobs, cargo-audit, npm-audit, name (+19 more)

### Community 25 - "Community 25"
Cohesion: 0.09
Nodes (13): loopback_up(), run(), setup(), enter(), map(), Mapping, Full, Single (+5 more)

### Community 26 - "Community 26"
Cohesion: 0.17
Nodes (4): check_name(), Engine, Helper, ImageRecord

### Community 27 - "Community 27"
Cohesion: 0.12
Nodes (22): base64(), BASE64, base64_matches_the_standard(), Basic, basic_auth(), HasVariables, Header, BASIC_WITH_VARS (+14 more)

### Community 28 - "Community 28"
Cohesion: 0.07
Nodes (26): app, security, windows, build, beforeBuildCommand, beforeDevCommand, devUrl, frontendDist (+18 more)

### Community 29 - "Community 29"
Cohesion: 0.12
Nodes (9): app(), AppState, bearer(), Config, DEMO_PASSWORD, index(), MAX_BODY, no_store() (+1 more)

### Community 30 - "Community 30"
Cohesion: 0.12
Nodes (13): GZIP_MAGIC, inside(), layer(), layer_reader(), never_writes_outside_the_root(), OPAQUE, owned(), reads_plain_tar_layers_too() (+5 more)

### Community 31 - "Community 31"
Cohesion: 0.23
Nodes (19): adds_default_headers_the_request_does_not_set(), env(), Environment, fills_url_headers_and_body_and_reports_secrets_used(), header(), knows_which_secrets_to_delete_from_the_keyring(), MAX_ID_LEN, requests_without_variables_need_no_environment() (+11 more)

### Community 32 - "Community 32"
Cohesion: 0.08
Nodes (24): concurrency, cancel-in-progress, group, environment, runs-on, steps, name, url (+16 more)

### Community 33 - "Community 33"
Cohesion: 0.16
Nodes (11): env_values(), FAILING_STATUS, NO_ENV, Outcome, Report, run(), run_checks(), run_one() (+3 more)

### Community 34 - "Community 34"
Cohesion: 0.13
Nodes (10): forward(), Pod, ContainerState, Reply, Connected, Error, Ok, Ready (+2 more)

### Community 35 - "Community 35"
Cohesion: 0.26
Nodes (17): children(), env_vars(), flatten(), is_export(), MAX_DEPTH, named_pairs(), on(), read() (+9 more)

### Community 36 - "Community 36"
Cohesion: 0.17
Nodes (9): delay(), echo(), MAX_DELAY_MS, MAX_REDIRECTS, PROXY_HEADERS, redirect(), sample_json(), status() (+1 more)

### Community 37 - "Community 37"
Cohesion: 0.10
Nodes (8): ContainerOptions, Forward, now_secs(), PullProgress, record_file(), record_files_are_safe(), REPLY_TIMEOUT, Mount

### Community 38 - "Community 38"
Cohesion: 0.18
Nodes (8): MemorySecrets, SecretError, SecretStore, entry(), explain(), KeyringSecrets, round_trip_in_the_system_keyring(), SERVICE

### Community 39 - "Community 39"
Cohesion: 0.17
Nodes (19): fills_in_known_names(), find_vars(), finds_each_name_once(), hides_the_longer_secret_first_and_skips_tiny_ones(), is_var_name(), leaves_text_that_only_looks_like_a_variable(), lists_every_unknown_name(), mask() (+11 more)

### Community 40 - "Community 40"
Cohesion: 0.15
Nodes (17): EnvFile, Environment, EnvList, listEnvironments(), saveEnvironments(), setSecret(), EnvPicker(), SecretReview() (+9 more)

### Community 41 - "Community 41"
Cohesion: 0.20
Nodes (17): Carry, Ctx, env_value(), environment(), exec(), fill_root(), find_program(), finds_env_values_last_one_wins() (+9 more)

### Community 42 - "Community 42"
Cohesion: 0.17
Nodes (16): react, CollectionView, ImportDone, TreeNode, CollectionTree(), MenuFor, Nodes(), parentOf() (+8 more)

### Community 43 - "Community 43"
Cohesion: 0.10
Nodes (19): compilerOptions, allowArbitraryExtensions, allowImportingTsExtensions, erasableSyntaxOnly, jsx, lib, module, moduleDetection (+11 more)

### Community 44 - "Community 44"
Cohesion: 0.13
Nodes (3): MAX_PACKET, recv(), send()

### Community 45 - "Community 45"
Cohesion: 0.19
Nodes (9): a_broken_file_locks_everything_down(), checks_urls_with_ip_addresses(), matches(), no_file_allows_everything(), Policy, policy(), reaches_the_ui_in_camel_case(), reads_the_rules() (+1 more)

### Community 46 - "Community 46"
Cohesion: 0.32
Nodes (17): binary_bodies_have_no_text(), fills_in_variables_and_hides_secrets_the_server_echoes(), http(), opts(), refuses_bad_methods_and_headers_before_sending(), says_when_nothing_is_listening(), says_when_the_host_does_not_exist(), says_when_the_server_is_too_slow() (+9 more)

### Community 47 - "Community 47"
Cohesion: 0.20
Nodes (13): ACCEPT, checks_digests(), Descriptor, Fetched, finish(), host_arch(), is_valid_digest(), Manifest (+5 more)

### Community 48 - "Community 48"
Cohesion: 0.19
Nodes (10): explain(), last_cause(), mask_response(), MAX_REDIRECTS, ms(), place_of(), Refused, Response (+2 more)

### Community 49 - "Community 49"
Cohesion: 0.19
Nodes (16): historyAdd(), historyClear(), HistoryEntry, historyList(), Response, HistoryList(), outcome(), when() (+8 more)

### Community 50 - "Community 50"
Cohesion: 0.16
Nodes (7): request_paths(), api(), bails_out_at_the_first_failure(), explains_missing_variables(), runs_a_collection_in_order_with_saved_values_and_checks(), TOKEN, write()

### Community 51 - "Community 51"
Cohesion: 0.12
Nodes (17): SendError, BadCertFile, BadHeader, BadMethod, BadProxy, BadUrl, Blocked, EmptyUrl (+9 more)

### Community 52 - "Community 52"
Cohesion: 0.12
Nodes (16): compilerOptions, allowImportingTsExtensions, erasableSyntaxOnly, lib, module, moduleDetection, noEmit, noFallthroughCasesInSwitch (+8 more)

### Community 53 - "Community 53"
Cohesion: 0.14
Nodes (5): DOCKER_HUB, DOCKER_HUB_API, ImageRef, parts(), reads_the_usual_forms()

### Community 54 - "Community 54"
Cohesion: 0.24
Nodes (13): from_text(), applying_twice_does_not_overwrite(), applying_writes_numbered_files_and_moves_secrets_to_the_keyring(), bruno_collection(), explains_what_it_cannot_read(), header(), POSTMAN, reads_a_bruno_collection() (+5 more)

### Community 55 - "Community 55"
Cohesion: 0.18
Nodes (6): bad_file(), Network, NO_PROXY, proxies_need_a_scheme(), PROXY_SCHEMES, read()

### Community 56 - "Community 56"
Cohesion: 0.13
Nodes (14): description, label, options, placeholder, value, body, attributes, id (+6 more)

### Community 57 - "Community 57"
Cohesion: 0.13
Nodes (14): CRANE_VAR_password, jobs, live, name, runs-on, steps, name, on (+6 more)

### Community 58 - "Community 58"
Cohesion: 0.23
Nodes (9): a_failing_service_takes_the_stack_down_with_its_log(), containers_end_when_the_pod_is_killed(), engine(), HELPER, processes(), runs_a_command_in_alpine(), runs_postgres_which_switches_user(), serves_http_through_a_forwarded_port() (+1 more)

### Community 59 - "Community 59"
Cohesion: 0.15
Nodes (12): BoxError, BadInput, Checksum, Io, Namespaces, Network, NoPlatform, Registry (+4 more)

### Community 60 - "Community 60"
Cohesion: 0.16
Nodes (4): Header, RequestSpec, MAX_SHOWN_BYTES, SendOptions

### Community 61 - "Community 61"
Cohesion: 0.23
Nodes (6): blocks_after_the_limit(), client_ip(), PRUNE_AT, RateLimit, trusts_only_the_last_forwarded_entry_and_only_behind_the_proxy(), WINDOW

### Community 62 - "Community 62"
Cohesion: 0.17
Nodes (11): rust, ui, patterns, interval, patterns, updates, directory, groups (+3 more)

### Community 63 - "Community 63"
Cohesion: 0.17
Nodes (11): description, label, body, attributes, id, type, validations, description (+3 more)

### Community 64 - "Community 64"
Cohesion: 0.25
Nodes (11): handle(), reap(), Running, serve(), stop_all(), Request, Connect, Run (+3 more)

### Community 66 - "Community 66"
Cohesion: 0.27
Nodes (6): CONTROL_FD, enter(), HELPER_ARG, remove(), report(), run_if_helper()

### Community 67 - "Community 67"
Cohesion: 0.22
Nodes (7): enter_root(), inside_path(), mnt(), mount_targets_stay_simple(), open_in_root(), real_dir(), fail()

### Community 68 - "Community 68"
Cohesion: 0.42
Nodes (4): EnvError, Invalid, Io, EnvFile

### Community 69 - "Community 69"
Cohesion: 0.39
Nodes (8): ImageView, ContainersView(), hostPortFor(), Logs(), nameFor(), PullForm(), RunForm(), StackPanel()

### Community 71 - "Community 71"
Cohesion: 0.52
Nodes (6): adds_http_for_local_addresses(), adds_https_for_everything_else(), is_local(), keeps_addresses_that_have_a_scheme(), normalize_url(), refuses_empty_spaced_and_other_schemes()

### Community 72 - "Community 72"
Cohesion: 0.43
Nodes (6): pickCertFile(), FileField(), modeOf(), NetworkFields(), ProxyMode, pickCertFile()

### Community 73 - "Community 73"
Cohesion: 0.43
Nodes (4): checkJson(), JsonProblem, scan(), Stop

### Community 74 - "Community 74"
Cohesion: 0.33
Nodes (5): description, identifier, permissions, $schema, windows

### Community 75 - "Community 75"
Cohesion: 0.67
Nodes (4): crane, crane-box, crane-cli, crane-core

## Knowledge Gaps
- **49 isolated node(s):** `Folder`, `Request`, `Help`, `Version`, `Full` (+44 more)
  These have ≤1 connection - possible missing edges. (Counts symbols only; 585 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **8 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `SecretStore` connect `Community 38` to `Community 33`, `Community 2`, `Community 3`, `Community 19`, `Community 31`?**
  _High betweenness centrality (0.013) - this node is a cross-community bridge._
- **Are the 2 inferred relationships involving `App()` (e.g. with `emptyDraft()` and `loadTheme()`) actually correct?**
  _`App()` has 2 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Folder`, `Request`, `Help` to the rest of the system?**
  _49 weakly-connected nodes found - possible missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.06915113871635611 - nodes in this community are weakly interconnected._
- **Why does `SendOptions` connect `Community 60` to `Community 33`, `Community 2`, `Community 3`, `Community 46`, `Community 48`, `Community 18`, `Community 50`?**
  _High betweenness centrality (0.011) - this node is a cross-community bridge._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.06459627329192547 - nodes in this community are weakly interconnected._
- **Why does `Engine` connect `Community 26` to `Community 17`, `Community 37`, `Community 14`?**
  _High betweenness centrality (0.010) - this node is a cross-community bridge._