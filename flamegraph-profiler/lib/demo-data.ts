import type { ProfileData } from "@/types/profiler";

/** Demo profile — mimics a real async Rust HTTP server (axum + tokio + sqlx) */
export const DEMO_PROFILE: ProfileData = {
  meta: {
    name: "axum-api-server",
    rustVersion: "1.78.0",
    durationNs: 30_000_000_000,
    sampleHz: 997,
    totalSamples: 29_910,
    peakHeapBytes: 48_234_496,
    tool: "cargo-flamegraph",
    capturedAt: new Date().toISOString(),
  },
  cpu: [
    // Root
    { name: "main",                              displayName: "main",                              crate: "axum-api", layer: "crate",   selfPct: 0.2,  totalPct: 100,  selfNs: 60_000_000,    totalNs: 30_000_000_000, depth: 0, x: 0,    w: 100,  samples: 29910 },
    // Tokio runtime
    { name: "tokio::runtime::Runtime::block_on", displayName: "tokio::runtime::Runtime::block_on",crate: "tokio",    layer: "runtime", selfPct: 1.4,  totalPct: 97.8, selfNs: 420_000_000,   totalNs: 29_340_000_000, depth: 1, x: 0,    w: 97.8, samples: 29251 },
    { name: "process::idle",                     displayName: "process::idle",                     crate: "kernel",   layer: "kernel",  selfPct: 2.2,  totalPct: 2.2,  selfNs: 660_000_000,   totalNs: 660_000_000,    depth: 1, x: 97.8, w: 2.2,  samples: 659   },
    // Event loop
    { name: "tokio::runtime::task::harness::poll_future", displayName: "tokio::runtime::task::harness::poll_future", crate: "tokio", layer: "runtime", selfPct: 2.1, totalPct: 62.4, selfNs: 630_000_000, totalNs: 18_720_000_000, depth: 2, x: 0, w: 62.4, samples: 18660 },
    { name: "tokio::runtime::io::driver::turn",  displayName: "tokio::runtime::io::driver::turn",  crate: "tokio",    layer: "runtime", selfPct: 1.8,  totalPct: 22.1, selfNs: 540_000_000,   totalNs: 6_630_000_000,  depth: 2, x: 62.4, w: 22.1, samples: 6611  },
    { name: "tokio::runtime::time::wheel",       displayName: "tokio::runtime::time::wheel",       crate: "tokio",    layer: "runtime", selfPct: 0.9,  totalPct: 13.3, selfNs: 270_000_000,   totalNs: 3_990_000_000,  depth: 2, x: 84.5, w: 13.3, samples: 3978  },
    // HTTP handler path
    { name: "axum::routing::Router::call_with_state", displayName: "axum::routing::Router::call_with_state", crate: "axum", layer: "dep", selfPct: 1.2, totalPct: 48.6, selfNs: 360_000_000, totalNs: 14_580_000_000, depth: 3, x: 0, w: 48.6, samples: 14534 },
    { name: "hyper::proto::h1::dispatch::Dispatcher::poll_catch", displayName: "hyper::proto::h1::dispatch::Dispatcher::poll_catch", crate: "hyper", layer: "dep", selfPct: 1.1, totalPct: 13.8, selfNs: 330_000_000, totalNs: 4_140_000_000, depth: 3, x: 48.6, w: 13.8, samples: 4127 },
    { name: "epoll_wait",                        displayName: "epoll_wait",                        crate: "kernel",   layer: "kernel",  selfPct: 18.2, totalPct: 20.1, selfNs: 5_460_000_000, totalNs: 6_030_000_000,  depth: 3, x: 62.4, w: 20.1, samples: 6011  },
    // Request handler
    { name: "axum_api::handlers::users::list_users::{{closure}}", displayName: "handlers::users::list_users", crate: "axum-api", layer: "crate", selfPct: 2.8, totalPct: 38.4, selfNs: 840_000_000, totalNs: 11_520_000_000, depth: 4, x: 0, w: 38.4, samples: 11487 },
    { name: "tower::util::map_response::MapResponse::call", displayName: "tower::util::map_response::MapResponse::call", crate: "tower", layer: "dep", selfPct: 0.8, totalPct: 10.2, selfNs: 240_000_000, totalNs: 3_060_000_000, depth: 4, x: 38.4, w: 10.2, samples: 3050 },
    // DB path
    { name: "sqlx::query::Query::fetch_all::{{closure}}", displayName: "sqlx::query::Query::fetch_all", crate: "sqlx", layer: "dep", selfPct: 3.2, totalPct: 22.6, selfNs: 960_000_000, totalNs: 6_780_000_000, depth: 5, x: 0, w: 22.6, samples: 6760 },
    { name: "axum_api::auth::verify_jwt::{{closure}}", displayName: "auth::verify_jwt", crate: "axum-api", layer: "crate", selfPct: 4.1, totalPct: 8.4, selfNs: 1_230_000_000, totalNs: 2_520_000_000, depth: 5, x: 22.6, w: 8.4, samples: 2513 },
    { name: "serde_json::value::from_value", displayName: "serde_json::value::from_value", crate: "serde_json", layer: "dep", selfPct: 5.8, totalPct: 7.2, selfNs: 1_740_000_000, totalNs: 2_160_000_000, depth: 5, x: 31, w: 7.2, samples: 2153 },
    // Crypto (deep hot path)
    { name: "ring::rsa::verification::verify::h3a8f92b1", displayName: "ring::rsa::verification::verify", crate: "ring", layer: "dep", selfPct: 3.8, totalPct: 18.4, selfNs: 1_140_000_000, totalNs: 5_520_000_000, depth: 6, x: 0, w: 18.4, samples: 5508 },
    { name: "sqlx::postgres::connection::PgConnection::run_query_sequence", displayName: "sqlx::postgres::PgConnection::run_query", crate: "sqlx", layer: "dep", selfPct: 4.4, totalPct: 14.2, selfNs: 1_320_000_000, totalNs: 4_260_000_000, depth: 6, x: 18.4, w: 14.2, samples: 4252 },
    { name: "serde::de::Deserialize::deserialize", displayName: "serde::de::Deserialize::deserialize", crate: "serde", layer: "dep", selfPct: 6.2, totalPct: 7.2, selfNs: 1_860_000_000, totalNs: 2_160_000_000, depth: 6, x: 32.6, w: 7.2, samples: 2153 },
    // OpenSSL / ring bottom
    { name: "EVP_DigestVerifyFinal",             displayName: "EVP_DigestVerifyFinal",             crate: "openssl",  layer: "kernel",  selfPct: 8.6, totalPct: 16.2, selfNs: 2_580_000_000, totalNs: 4_860_000_000, depth: 7, x: 0, w: 16.2, samples: 4847 },
    { name: "postgres_protocol::message::Message::parse", displayName: "postgres_protocol::message::Message::parse", crate: "postgres-protocol", layer: "dep", selfPct: 7.1, totalPct: 11.8, selfNs: 2_130_000_000, totalNs: 3_540_000_000, depth: 7, x: 16.2, w: 11.8, samples: 3531 },
    { name: "BN_mod_exp_mont",                   displayName: "BN_mod_exp_mont",                   crate: "openssl",  layer: "kernel",  selfPct: 10.4, totalPct: 14.8, selfNs: 3_120_000_000, totalNs: 4_440_000_000, depth: 8, x: 0, w: 14.8, samples: 4425 },
    { name: "RSA_verify",                        displayName: "RSA_verify",                        crate: "openssl",  layer: "kernel",  selfPct: 7.2, totalPct: 9.4, selfNs: 2_160_000_000, totalNs: 2_820_000_000, depth: 9, x: 0, w: 9.4, samples: 2811 },
  ],
  alloc: [
    { name: "main",                              displayName: "main",                              crate: "axum-api", layer: "crate",   selfPct: 0,    totalPct: 100,  selfNs: 0,             totalNs: 0,              depth: 0, x: 0,    w: 100,  samples: 52441 },
    { name: "__rdl_alloc",                       displayName: "__rdl_alloc (global allocator)",    crate: "alloc",    layer: "alloc",   selfPct: 24.2, totalPct: 74.8, selfNs: 0,             totalNs: 0,              depth: 1, x: 0,    w: 74.8, samples: 39282 },
    { name: "std::rt::lang_start::gc",           displayName: "GC / drop chain",                  crate: "std",      layer: "std",     selfPct: 25.2, totalPct: 25.2, selfNs: 0,             totalNs: 0,              depth: 1, x: 74.8, w: 25.2, samples: 13222 },
    { name: "axum_api::handlers::users::list_users::{{closure}}", displayName: "handlers::users::list_users", crate: "axum-api", layer: "crate", selfPct: 8.2, totalPct: 44.1, selfNs: 0, totalNs: 0, depth: 2, x: 0, w: 44.1, samples: 23152, allocBytes: 18_432_000 },
    { name: "serde_json::from_slice",            displayName: "serde_json::from_slice",            crate: "serde_json", layer: "dep",  selfPct: 18.4, totalPct: 22.8, selfNs: 0,             totalNs: 0,              depth: 2, x: 44.1, w: 22.8, samples: 11968, allocBytes: 9_216_000 },
    { name: "Vec::push",                         displayName: "Vec::push (collection growth)",     crate: "alloc",    layer: "std",     selfPct: 12.4, totalPct: 20.1, selfNs: 0,             totalNs: 0,              depth: 3, x: 0,    w: 20.1, samples: 10554, allocBytes: 8_192_000 },
    { name: "String::from_utf8",                 displayName: "String::from_utf8",                 crate: "std",      layer: "std",     selfPct: 9.8,  totalPct: 14.4, selfNs: 0,             totalNs: 0,              depth: 3, x: 20.1, w: 14.4, samples: 7558, allocBytes: 5_898_240 },
    { name: "axum_api::models::User::from_row",  displayName: "User::from_row (ORM mapping)",      crate: "axum-api", layer: "crate",   selfPct: 14.2, totalPct: 18.6, selfNs: 0,             totalNs: 0,              depth: 3, x: 34.5, w: 18.6, samples: 9763, allocBytes: 7_602_176, arcClones: 18420 },
    { name: "Box::new",                          displayName: "Box::new (dyn Error boxing)",       crate: "alloc",    layer: "std",     selfPct: 8.1,  totalPct: 8.4,  selfNs: 0,             totalNs: 0,              depth: 4, x: 0,    w: 8.4,  samples: 4409 },
  ],
  offcpu: [
    { name: "main",                              displayName: "main",                              crate: "axum-api", layer: "crate",   selfPct: 0,    totalPct: 100,  selfNs: 0,             totalNs: 0,              depth: 0, x: 0,    w: 100,  samples: 8312, isAsync: false },
    { name: "syscall::blocked",                  displayName: "syscall (blocked)",                 crate: "kernel",   layer: "kernel",  selfPct: 4.2,  totalPct: 86.4, selfNs: 0,             totalNs: 0,              depth: 1, x: 0,    w: 86.4, samples: 7182, isAsync: false },
    { name: "tokio::park",                       displayName: "tokio::park (worker idle)",         crate: "tokio",    layer: "runtime", selfPct: 13.6, totalPct: 13.6, selfNs: 0,             totalNs: 0,              depth: 1, x: 86.4, w: 13.6, samples: 1130, isAsync: true  },
    { name: "epoll_wait",                        displayName: "epoll_wait (I/O blocked)",          crate: "kernel",   layer: "kernel",  selfPct: 42.1, totalPct: 42.1, selfNs: 0,             totalNs: 0,              depth: 2, x: 0,    w: 42.1, samples: 3501, isAsync: false },
    { name: "futex_wait",                        displayName: "futex_wait (mutex contention)",     crate: "kernel",   layer: "kernel",  selfPct: 24.8, totalPct: 24.8, selfNs: 0,             totalNs: 0,              depth: 2, x: 42.1, w: 24.8, samples: 2062, isAsync: false },
    { name: "read / write syscall",              displayName: "read / write (network I/O)",        crate: "kernel",   layer: "kernel",  selfPct: 14.4, totalPct: 19.5, selfNs: 0,             totalNs: 0,              depth: 2, x: 66.9, w: 19.5, samples: 1621, isAsync: false },
    { name: "sqlx::pool::inner::SharedPool::acquire::{{closure}}", displayName: "sqlx::pool::SharedPool::acquire", crate: "sqlx", layer: "dep", selfPct: 18.2, totalPct: 22.4, selfNs: 0, totalNs: 0, depth: 3, x: 0, w: 22.4, samples: 1862, isAsync: true, pollCount: 14820 },
    { name: "axum_api::auth::verify_jwt::{{closure}}", displayName: "auth::verify_jwt (lock wait)", crate: "axum-api", layer: "crate", selfPct: 16.4, totalPct: 18.6, selfNs: 0, totalNs: 0, depth: 3, x: 22.4, w: 18.6, samples: 1547, isAsync: true, pollCount: 9240 },
    { name: "reqwest::async_impl::client::Client::execute", displayName: "reqwest::Client::execute (external API)", crate: "reqwest", layer: "dep", selfPct: 9.4, totalPct: 11.2, selfNs: 0, totalNs: 0, depth: 3, x: 41, w: 11.2, samples: 931, isAsync: true, pollCount: 4410 },
    { name: "tracing::span::Span::exit",         displayName: "tracing::Span::exit (lock contend)", crate: "tracing", layer: "dep",   selfPct: 8.1,  totalPct: 9.4,  selfNs: 0,             totalNs: 0,              depth: 3, x: 52.2, w: 9.4,  samples: 781,  isAsync: false },
    { name: "sqlx::postgres::PgConnection::run", displayName: "sqlx: SQL SELECT users",            crate: "sqlx",     layer: "dep",   selfPct: 14.8, totalPct: 18.4, selfNs: 0,             totalNs: 0,              depth: 4, x: 0,    w: 18.4, samples: 1530, isAsync: true  },
    { name: "deadpool::managed::Pool::get",      displayName: "deadpool: pool exhausted wait",     crate: "deadpool", layer: "dep",   selfPct: 7.8,  totalPct: 10.2, selfNs: 0,             totalNs: 0,              depth: 4, x: 18.4, w: 10.2, samples: 848,  isAsync: true  },
  ],
};
