# Shared hardware readings: implementation contract
*Document type: reference. It describes how the part works. For the current state, read [status](../status.md).*

Plan 2.6a, completed software implementation 2026-11-05. Installed verification belongs in acceptance.
UC owns one demand-driven collector. Dock and cooler request only hardware readings their enabled displays consume.
Music, weather, AI usage, disks/network and external/admin sensor integrations retain their existing ownership for this slice.

## Wire version 1

Wire request, Demand, Snapshot and Gpu objects reject unknown fields. Their JSON names use snake_case in both repositories.

`Demand`: `{ "cpu": false, "ram": false, "gpu": false, "hz": 1 }`.
Boolean fields select CPU load, RAM used/total and the complete existing NVIDIA GPU readings respectively.
`hz` is 1, 2 or 4; the collector uses the union of demand and the fastest requested rate. No fields requested means no reads.

`Snapshot`: `{ "version":1, "generation":1, "sequence":1, "observed_at_ms":0, "expires_at_ms":0,
"cpu_load_pct":null, "ram_used_bytes":null, "ram_total_bytes":null, "gpu":null }`.

`Gpu`: `{ "name":"", "temp_c":null, "load_pct":null, "clock_mhz":null, "mem_used_bytes":null,
"mem_total_bytes":null, "power_w":null, "fan_pct":null }`.

GPU name is a product label of at most 128 printable Unicode characters, not a device identifier. All values are finite:
percentages 0–100, temperature−40–200°C, clock 0–20000 MHz, power 0–10000 W. Memory fields are paired used/total values,
total greater than zero and at most 2^50 bytes; used cannot exceed total. Missing/failed fields remain null. Protocol IDs,
sequence and times are positive JavaScript-safe integers. No CPU-temperature driver is added.
The first CPU delta is unknown. Snapshots carry their original observation timestamp and expire within three seconds.
Future, malformed, expired, duplicate or older sequence data must not refresh a reading. Generation is scoped to the
current host session/channel and binds on first accepted snapshot; a replacement worker begins empty and cannot replay
a previous worker's state. A different generation cannot replace a bound channel's current reading.

The dock requests hardware for both faces of keys on the delivered page. Its unchanged AI policy remains per active
profile. Cooler demand follows actual visible legacy/modern widget reads, with two-second rate leases. Host leases
expire after three seconds without a successful worker exchange; unneeded source families release immediately.

## Worker exchange

Cooler is launched with `UC_SHARED_HARDWARE=1` by UC only. Standalone CoolerScreen keeps its local collector mode.
The existing validated private worker RPC gains `hardware_exchange`, args `{ "snapshot": null }` or a version 1 snapshot.
It accepts the snapshot and returns `{ "demand": Demand, "state": ... }`; state may contain finite counters/flags for
quiet verification, never payloads, paths or device identifiers. This operation performs no device writes or native reads.
State retains original `expires_at_ms`, accepted sequence and actual local-poll counters. `gpu_fields` is an availability
bitmask: temperature 1, load 2, clock 4, memory-used 8, memory-total 16, power 32, fan 64, name 128; 255 means all fields are available.
The dock protocol gets an equivalent typed operation, with shared mode declared in Init. Both worker implementations
consume injected readings and disable replaced local CPU/RAM/NVML collection when shared mode is active.

UC exchanges with each worker independently, coalescing to the latest snapshot. A stalled consumer cannot block collection
or delivery to the other worker. Leases expire after worker loss; demand is removed on disable/stop. Reenable/recovery
requests current demand and delivers a fresh result, without reactivating an unneeded source or duplicate collector.
Workers show unknown after expiry/provider failure; no local fallback silently duplicates a shared reader.
Shared sensor pixels retain the original deadline through dock batch rendering, private IPC and physical delivery guards.
Ephemeral exchanges use bounded single-probe gates, independent of durable-action uncertainty and busy-safe Quit.

## Completion

Cover union demand/rates, no consumers/no reads, disabling one consumer while the other remains, missing/expired values,
sequence/generation recovery, existing cooler detail preservation, one collector and disabled local collector counters,
independent worker crash/recovery and busy-safe Quit. Tests use quiet isolated debug/fake devices and fake sensor sources.

Software checks:179 UC core/11 host/62 reader cases, seven focused cooler shared-hardware cases and four worker cases;
the quiet actual-process shared-hardware fixture covers both consumers, no local duplicates, complete GPU fields,
failure/paused-delivery expiry/recovery, crash isolation, demand release and safe Quit. Browser/native lifecycle checks
remain separate from physical readability and long-running reliability acceptance.
