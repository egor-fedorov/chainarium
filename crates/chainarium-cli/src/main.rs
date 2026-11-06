use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as ApiPath, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use chainarium_core::{Amount, Run, Scenario, run::save_checkpoint};
use serde::Deserialize;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

type Shared = Arc<Mutex<Option<Run>>>;
type ApiError = (StatusCode, String);
fn bad(e: impl ToString) -> ApiError {
    (StatusCode::BAD_REQUEST, e.to_string())
}
fn current(s: &Shared) -> Result<std::sync::MutexGuard<'_, Option<Run>>, ApiError> {
    s.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "controller unavailable".into(),
        )
    })
}
async fn state(State(s): State<Shared>) -> Result<Json<Run>, ApiError> {
    Ok(Json(
        current(&s)?.as_ref().ok_or_else(|| bad("no run"))?.clone(),
    ))
}
async fn create(State(s): State<Shared>, bytes: Bytes) -> Result<Json<Run>, ApiError> {
    let run = Run::new(Scenario::parse(&bytes).map_err(bad)?).map_err(bad)?;
    *current(&s)? = Some(run.clone());
    Ok(Json(run))
}
async fn step(State(s): State<Shared>) -> Result<Json<Run>, ApiError> {
    let mut g = current(&s)?;
    let r = g.as_mut().ok_or_else(|| bad("no run"))?;
    r.step().map_err(bad)?;
    Ok(Json(r.clone()))
}
async fn checkpoint(State(s): State<Shared>) -> Result<impl IntoResponse, ApiError> {
    let bytes = current(&s)?
        .as_ref()
        .ok_or_else(|| bad("no run"))?
        .checkpoint()
        .map_err(bad)?;
    Ok(([(header::CONTENT_TYPE, "application/json")], bytes))
}
async fn restore(State(s): State<Shared>, bytes: Bytes) -> Result<Json<Run>, ApiError> {
    let r = Run::restore(&bytes).map_err(bad)?;
    *current(&s)? = Some(r.clone());
    Ok(Json(r))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fork {
    label: String,
}
async fn fork(State(s): State<Shared>, Json(input): Json<Fork>) -> Result<Json<Run>, ApiError> {
    let mut g = current(&s)?;
    let r = g
        .as_ref()
        .ok_or_else(|| bad("no run"))?
        .fork(&input.label)
        .map_err(bad)?;
    *g = Some(r.clone());
    Ok(Json(r))
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    amount: Amount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price: Option<chainarium_core::amount::Price>,
}
async fn wire(Json(value): Json<Wire>) -> Json<Wire> {
    Json(value)
}
async fn preset(ApiPath(id): ApiPath<String>) -> Result<impl IntoResponse, ApiError> {
    let data = match id.as_str() {
        "sc-mtm-capital" => include_str!("../../../scenarios/demos/sc-mtm-capital.json"),
        "sc-mtm-unpriced" => include_str!("../../../scenarios/demos/sc-mtm-unpriced.json"),
        "sc-mtm-flash" => include_str!("../../../scenarios/demos/sc-mtm-flash.json"),
        "sc-mtm-credit" => include_str!("../../../scenarios/demos/sc-mtm-credit.json"),
        "sc-mtm-clmm" => include_str!("../../../scenarios/demos/sc-mtm-clmm.json"),
        "sc-mtm-book" => include_str!("../../../scenarios/demos/sc-mtm-book.json"),
        "sc-flash-arb-success" => {
            include_str!("../../../scenarios/demos/sc-flash-arb-success.json")
        }
        "sc-flash-arb-unprofitable" => {
            include_str!("../../../scenarios/demos/sc-flash-arb-unprofitable.json")
        }
        "sc-flash-arb-guard" => include_str!("../../../scenarios/demos/sc-flash-arb-guard.json"),
        "sc-flash-arb-shortfall" => {
            include_str!("../../../scenarios/demos/sc-flash-arb-shortfall.json")
        }
        "sc-flash-arb-delayed" => {
            include_str!("../../../scenarios/demos/sc-flash-arb-delayed.json")
        }
        "sc-flash-arb-budget" => include_str!("../../../scenarios/demos/sc-flash-arb-budget.json"),
        "sc-scheduler-public" => include_str!("../../../scenarios/demos/sc-scheduler-public.json"),
        "sc-scheduler-private" => {
            include_str!("../../../scenarios/demos/sc-scheduler-private.json")
        }
        "sc-scheduler-delayed" => {
            include_str!("../../../scenarios/demos/sc-scheduler-delayed.json")
        }
        "sc-scheduler-nonce" => include_str!("../../../scenarios/demos/sc-scheduler-nonce.json"),
        "sc-scheduler-fifo" => include_str!("../../../scenarios/demos/sc-scheduler-fifo.json"),
        "sc-scheduler-fee" => include_str!("../../../scenarios/demos/sc-scheduler-fee.json"),
        "sc-agent-random" => include_str!("../../../scenarios/demos/sc-agent-random.json"),
        "sc-agent-scripted" => include_str!("../../../scenarios/demos/sc-agent-scripted.json"),
        "sc-agent-preflight" => include_str!("../../../scenarios/demos/sc-agent-preflight.json"),
        "sc-agent-borrower" => include_str!("../../../scenarios/demos/sc-agent-borrower.json"),
        "sc-agent-spot" => include_str!("../../../scenarios/demos/sc-agent-spot.json"),
        "sc-agent-twap" => include_str!("../../../scenarios/demos/sc-agent-twap.json"),
        "sc-hook-fee" => include_str!("../../../scenarios/demos/sc-hook-fee.json"),
        "sc-hook-limit" => include_str!("../../../scenarios/demos/sc-hook-limit.json"),
        "sc-hook-rollback" => include_str!("../../../scenarios/demos/sc-hook-rollback.json"),
        "sc-hook-shortfall" => include_str!("../../../scenarios/demos/sc-hook-shortfall.json"),
        "sc-hook-lp" => include_str!("../../../scenarios/demos/sc-hook-lp.json"),
        "sc-hook-route" => include_str!("../../../scenarios/demos/sc-hook-route.json"),
        "sc-liquidator-own" => include_str!("../../../scenarios/demos/sc-liquidator-own.json"),
        "sc-liquidator-flash" => include_str!("../../../scenarios/demos/sc-liquidator-flash.json"),
        "sc-liquidator-fifo" => include_str!("../../../scenarios/demos/sc-liquidator-fifo.json"),
        "sc-liquidator-fee" => include_str!("../../../scenarios/demos/sc-liquidator-fee.json"),
        "sc-liquidator-stale" => include_str!("../../../scenarios/demos/sc-liquidator-stale.json"),
        "sc-liquidator-delayed" => {
            include_str!("../../../scenarios/demos/sc-liquidator-delayed.json")
        }
        "sc-backrun-public" => include_str!("../../../scenarios/demos/sc-backrun-public.json"),
        "sc-backrun-private" => include_str!("../../../scenarios/demos/sc-backrun-private.json"),
        "sc-backrun-delayed" => include_str!("../../../scenarios/demos/sc-backrun-delayed.json"),
        "sc-backrun-fifo" => include_str!("../../../scenarios/demos/sc-backrun-fifo.json"),
        "sc-backrun-competition" => {
            include_str!("../../../scenarios/demos/sc-backrun-competition.json")
        }
        "sc-backrun-fees" => include_str!("../../../scenarios/demos/sc-backrun-fees.json"),
        "sc-bundle-allowed" => include_str!("../../../scenarios/demos/sc-bundle-allowed.json"),
        "sc-bundle-forbidden" => include_str!("../../../scenarios/demos/sc-bundle-forbidden.json"),
        "sc-bundle-greedy" => include_str!("../../../scenarios/demos/sc-bundle-greedy.json"),
        "sc-bundle-fee" => include_str!("../../../scenarios/demos/sc-bundle-fee.json"),
        "sc-bundle-visible" => include_str!("../../../scenarios/demos/sc-bundle-visible.json"),
        "sc-bundle-private" => include_str!("../../../scenarios/demos/sc-bundle-private.json"),
        "sc-01" => include_str!("../../../scenarios/demos/sc-01.json"),
        "sc-lp" => include_str!("../../../scenarios/demos/sc-lp.json"),
        "sc-03-route" => include_str!("../../../scenarios/demos/sc-03-route.json"),
        "sc-lp-policies" => include_str!("../../../scenarios/demos/sc-lp-policies.json"),
        "sc-mm-oracle" => include_str!("../../../scenarios/demos/sc-mm-oracle.json"),
        "sc-03-heterogeneous" => include_str!("../../../scenarios/demos/sc-03-heterogeneous.json"),
        "sc-04-flash" => include_str!("../../../scenarios/demos/sc-04-flash.json"),
        "sc-05-credit" => include_str!("../../../scenarios/demos/sc-05-credit.json"),
        "sc-stable" => include_str!("../../../scenarios/demos/sc-stable.json"),
        "sc-stable-depeg" => include_str!("../../../scenarios/demos/sc-stable-depeg.json"),
        "sc-07-flash-liquidation" => {
            include_str!("../../../scenarios/demos/sc-07-flash-liquidation.json")
        }
        "sc-02-clmm" => include_str!("../../../scenarios/demos/sc-02-clmm.json"),
        "sc-clmm-crossing" => include_str!("../../../scenarios/demos/sc-clmm-crossing.json"),
        "sc-08-book" => include_str!("../../../scenarios/demos/sc-08-book.json"),
        "sc-twap" => include_str!("../../../scenarios/demos/sc-twap.json"),
        "sc-twap-120" => include_str!("../../../scenarios/demos/sc-twap-120.json"),
        _ => return Err((StatusCode::NOT_FOUND, "unknown preset".into())),
    };
    Ok(([(header::CONTENT_TYPE, "application/json")], data))
}
#[derive(serde::Serialize)]
struct CreditView {
    run_id: String,
    block: Amount,
    markets: Vec<chainarium_core::credit::MarketView>,
}
#[derive(serde::Serialize)]
struct OracleView {
    run_id: String,
    block: Amount,
    feeds: Vec<chainarium_core::twap::FeedView>,
}
#[derive(serde::Serialize)]
struct BooksView {
    run_id: String,
    block: Amount,
    books: Vec<chainarium_core::book::View>,
}
#[derive(serde::Serialize)]
struct ClmmView {
    run_id: String,
    block: Amount,
    pools: Vec<chainarium_core::clmm::View>,
}
async fn clmm_view(
    State(s): State<Shared>,
    ApiPath(block): ApiPath<usize>,
) -> Result<Json<ClmmView>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    Ok(Json(ClmmView {
        run_id: r.run_id.clone(),
        block: w.block,
        pools: chainarium_core::clmm::views(&r.scenario, w).map_err(bad)?,
    }))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClmmQuoteRequest {
    block: usize,
    agent: String,
    sender: String,
    pool: String,
    asset_in: String,
    amount: Amount,
    limit_tick: i32,
    min_out: Amount,
    allow_partial: bool,
}
async fn clmm_quote(
    State(s): State<Shared>,
    Json(input): Json<ClmmQuoteRequest>,
) -> Result<Json<chainarium_core::clmm::Quote>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if input.block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(input.block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    let nonce = w
        .accounts
        .get(&input.sender)
        .ok_or_else(|| bad("unknown sender"))?
        .nonce;
    let tx = chainarium_core::Transaction {
        id: "readonly-clmm-quote".into(),
        agent: input.agent,
        sender: input.sender,
        chain: r.scenario.chain.id.clone(),
        nonce,
        limit: 1000.into(),
        price: Amount::ZERO,
        deadline: w.timestamp,
        program: vec![chainarium_core::Call::ClmmSwap {
            pool: input.pool,
            asset_in: input.asset_in,
            amount: input.amount,
            sqrt_price_limit: chainarium_core::clmm::math::tick_sqrt(input.limit_tick)
                .map_err(bad)?,
            min_out: input.min_out,
            allow_partial: input.allow_partial,
        }],
    };
    Ok(Json(
        chainarium_core::clmm::quote(&r.scenario, w, &tx).map_err(bad)?,
    ))
}
async fn books_view(
    State(s): State<Shared>,
    ApiPath(block): ApiPath<usize>,
) -> Result<Json<BooksView>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    Ok(Json(BooksView {
        run_id: r.run_id.clone(),
        block: w.block,
        books: chainarium_core::book::views(&r.scenario, w).map_err(bad)?,
    }))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BookQuoteRequest {
    block: usize,
    transaction: chainarium_core::Transaction,
}
async fn book_quote(
    State(s): State<Shared>,
    Json(input): Json<BookQuoteRequest>,
) -> Result<Json<chainarium_core::book::Quote>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if input.block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(input.block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    Ok(Json(
        chainarium_core::book::quote(&r.scenario, w, &input.transaction).map_err(bad)?,
    ))
}
async fn oracle_view(
    State(s): State<Shared>,
    ApiPath(block): ApiPath<usize>,
) -> Result<Json<OracleView>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    Ok(Json(OracleView {
        run_id: r.run_id.clone(),
        block: w.block,
        feeds: chainarium_core::twap::views(&r.scenario, w).map_err(bad)?,
    }))
}
#[derive(serde::Serialize)]
struct MarketView {
    run_id: String,
    block: Amount,
    market: chainarium_core::inspection::MarketView,
}
async fn market_view(
    State(s): State<Shared>,
    ApiPath((block, id)): ApiPath<(usize, String)>,
) -> Result<Json<MarketView>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    Ok(Json(MarketView {
        run_id: r.run_id.clone(),
        block: w.block,
        market: chainarium_core::inspection::market(&r.scenario, w, &id).map_err(bad)?,
    }))
}
async fn credit_view(
    State(s): State<Shared>,
    ApiPath(block): ApiPath<usize>,
) -> Result<Json<CreditView>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    Ok(Json(CreditView {
        run_id: r.run_id.clone(),
        block: w.block,
        markets: chainarium_core::credit::views(&r.scenario, w),
    }))
}
async fn valuation_view(
    State(s): State<Shared>,
    ApiPath(block): ApiPath<usize>,
) -> Result<Json<chainarium_core::valuation::Boundary>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    Ok(Json(chainarium_core::valuation::at(r, block).map_err(bad)?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuoteRequest {
    account: String,
    asset_in: String,
    asset_out: String,
    amount: Amount,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteQuoteRequest {
    block: usize,
    query: chainarium_core::routing::heterogeneous::Query,
}
async fn route_quote(
    State(s): State<Shared>,
    Json(input): Json<RouteQuoteRequest>,
) -> Result<Json<chainarium_core::routing::heterogeneous::Quote>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let w = if input.block == 0 {
        &r.initial
    } else {
        &r.blocks
            .get(input.block - 1)
            .ok_or_else(|| bad("unknown block"))?
            .state
    };
    Ok(Json(
        chainarium_core::routing::heterogeneous::quote(&r.scenario, w, &input.query)
            .map_err(bad)?,
    ))
}
async fn routes(
    State(s): State<Shared>,
    Json(input): Json<QuoteRequest>,
) -> Result<Json<chainarium_core::routing::Search>, ApiError> {
    let g = current(&s)?;
    let r = g.as_ref().ok_or_else(|| bad("no run"))?;
    let obs = chainarium_core::routing::Observation::capture(&r.scenario, &r.state, &input.account)
        .map_err(bad)?;
    Ok(Json(chainarium_core::routing::search(
        &obs,
        &input.asset_in,
        &input.asset_out,
        input.amount,
    )))
}
fn app() -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../../../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/app.js"),
                )
            }),
        )
        .route(
            "/wire.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/wire.mjs"),
                )
            }),
        )
        .route(
            "/flash-arbitrage.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/flash-arbitrage.js"),
                )
            }),
        )
        .route(
            "/valuation.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/valuation.js"),
                )
            }),
        )
        .route(
            "/agents.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/agents.js"),
                )
            }),
        )
        .route(
            "/hooks.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/hooks.js"),
                )
            }),
        )
        .route(
            "/liquidators.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/liquidators.js"),
                )
            }),
        )
        .route(
            "/bundles.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/bundles.js"),
                )
            }),
        )
        .route(
            "/scheduler.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/scheduler.js"),
                )
            }),
        )
        .route(
            "/policies.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/policies.js"),
                )
            }),
        )
        .route(
            "/routes.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/routes.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../../../web/style.css"),
                )
            }),
        )
        .route(
            "/api/preset",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/json")],
                    include_str!("../../../scenarios/demos/sc-01.json"),
                )
            }),
        )
        .route("/api/preset/{id}", get(preset))
        .route("/api/routes", post(routes))
        .route("/api/route-quote", post(route_quote))
        .route("/api/credit/{block}", get(credit_view))
        .route("/api/valuation/{block}", get(valuation_view))
        .route("/api/market/{block}/{id}", get(market_view))
        .route("/api/oracles/{block}", get(oracle_view))
        .route("/api/books/{block}", get(books_view))
        .route("/api/book-quote", post(book_quote))
        .route("/api/clmm/{block}", get(clmm_view))
        .route("/api/clmm-quote", post(clmm_quote))
        .route("/api/run", get(state).post(create))
        .route("/api/step", post(step))
        .route("/api/checkpoint", get(checkpoint))
        .route(
            "/api/restore",
            post(restore).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route("/api/fork", post(fork))
        .route("/api/wire", post(wire))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(Arc::new(Mutex::new(None)))
}
fn load(path: &str) -> Result<Scenario, String> {
    Scenario::parse(&std::fs::read(path).map_err(|e| e.to_string())?)
}
fn required(args: &[String], i: usize) -> Result<&str, String> {
    args.get(i)
        .map(String::as_str)
        .ok_or_else(|| "missing argument; use --help".into())
}
#[tokio::main]
async fn main() {
    if let Err(e) = command().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
async fn command() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str).unwrap_or("--help") {
        "routes" => {
            let mut r = Run::new(load(required(&args, 1)?)?)?;
            let n: usize = required(&args, 2)?
                .parse()
                .map_err(|_| "invalid block count")?;
            if n > r.scenario.rounds() {
                return Err("block count exceeds scenario".into());
            }
            for _ in 0..n {
                r.step()?;
            }
            let bytes = std::fs::read(required(&args, 3)?).map_err(|e| e.to_string())?;
            if bytes.len() > chainarium_core::model::MAX_INPUT {
                return Err("route query exceeds 1 MiB".into());
            }
            let q = serde_json::from_slice(&bytes).map_err(|e| format!("route query: {e}"))?;
            let result = chainarium_core::routing::heterogeneous::quote(&r.scenario, &r.state, &q)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
            );
        }
        "run" => {
            let mut r = Run::new(load(required(&args, 1)?)?)?;
            r.finish()?;
            r.export(Path::new(required(&args, 2)?))?;
            println!("{} committed blocks exported", r.blocks.len());
        }
        "checkpoint" => {
            let mut r = Run::new(load(required(&args, 1)?)?)?;
            let n: usize = required(&args, 2)?
                .parse()
                .map_err(|_| "invalid block count")?;
            if n > r.scenario.rounds() {
                return Err("block count exceeds scenario".into());
            }
            for _ in 0..n {
                r.step()?;
            }
            save_checkpoint(Path::new(required(&args, 3)?), &r)?;
            println!("Checkpoint after {n} blocks saved");
        }
        "resume" => {
            let bytes = std::fs::read(required(&args, 1)?).map_err(|e| e.to_string())?;
            let mut r = Run::restore(&bytes)?;
            r.finish()?;
            r.export(Path::new(required(&args, 2)?))?;
            println!("Restored and completed {} blocks", r.blocks.len());
        }
        "serve" => {
            let port: u16 = args
                .get(1)
                .map(String::as_str)
                .unwrap_or("8080")
                .parse()
                .map_err(|_| "invalid port")?;
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                .await
                .map_err(|e| e.to_string())?;
            println!("Chainarium M1–M4g inspector listening on port {port}");
            axum::serve(listener, app())
                .await
                .map_err(|e| e.to_string())?;
        }
        "benchmark" => {
            use chainarium_core::{Call, Transaction};
            let blocks: usize = args
                .get(1)
                .map(String::as_str)
                .unwrap_or("1000")
                .parse()
                .map_err(|_| "invalid block count")?;
            if blocks > 10000 {
                return Err("benchmark limit: 10000 blocks".into());
            }
            let mut s = load("scenarios/demos/sc-01.json")?;
            s.blocks.clear();
            s.chain.block_limit = 10000.into();
            // M1 scaled CPMM workload: 50 scripted senders, 4 CPMMs, 50 candidates/block.
            // No future agents or other market families are imitated.
            use chainarium_core::{Account, Agent};
            use std::collections::{BTreeMap, BTreeSet};
            for j in 0..50 {
                let id = format!("agent-{j:02}");
                s.accounts.insert(
                    id.clone(),
                    Account {
                        nonce: Amount::ZERO,
                        balances: BTreeMap::from([
                            ("X".into(), 1_000_000_000.into()),
                            ("Q".into(), 1_000_000_000.into()),
                        ]),
                    },
                );
                s.agents.insert(
                    id.clone(),
                    Agent {
                        controlled_accounts: BTreeSet::from([id]),
                    },
                );
            }
            let pool = s.protocols["cpmm"].clone();
            s.protocols.clear();
            for j in 0..4 {
                let id = format!("pool-{j}");
                let mut p = pool.clone();
                p.account = id.clone();
                s.accounts.insert(
                    id.clone(),
                    Account {
                        nonce: Amount::ZERO,
                        balances: BTreeMap::from([
                            ("X".into(), 1_000_000.into()),
                            ("Q".into(), 1_000_000.into()),
                        ]),
                    },
                );
                s.protocols.insert(id, p);
            }
            for i in 0..blocks {
                let mut txs = vec![];
                for j in 0..50 {
                    txs.push(Transaction {
                        id: format!("bench-{i}-{j}"),
                        agent: format!("agent-{j:02}"),
                        sender: format!("agent-{j:02}"),
                        chain: "lab-1".into(),
                        nonce: (i as u64).into(),
                        limit: 200.into(),
                        price: Amount::ZERO,
                        deadline: Amount::MAX,
                        program: vec![Call::SwapExactIn {
                            pool: format!("pool-{}", j % 4),
                            asset_in: if (i + j) % 2 == 0 { "X" } else { "Q" }.into(),
                            amount: 100.into(),
                            min_out: 1.into(),
                        }],
                    });
                }
                s.blocks.push(txs);
            }
            let mut r = Run::new(s)?;
            let start = std::time::Instant::now();
            r.finish()?;
            let n = r.blocks.iter().map(|b| b.receipts.len()).sum::<usize>();
            let elapsed = start.elapsed().as_secs_f64();
            if n != blocks * 50
                || r.blocks
                    .iter()
                    .flat_map(|b| &b.receipts)
                    .any(|r| r.status != "Success")
            {
                return Err("benchmark did not execute all expected swaps".into());
            }

            println!(
                "{}",
                serde_json::json!({"profile":"M1-CPMM-BASELINE","blocks":blocks.to_string(),"receipts":n.to_string(),"seconds":elapsed,"tx_per_second":n as f64/elapsed,"build":if cfg!(debug_assertions){"debug"}else{"release"},"workers":1,"traces":"complete","checkpoint_bytes":r.checkpoint()?.len().to_string()})
            );
        }
        "--help" | "help" => println!(
            "Chainarium M1–M4g (v1.0 in development)\n  run SCENARIO NEW_OUTPUT_DIR\n  routes SCENARIO BLOCKS QUERY_JSON\n  checkpoint SCENARIO BLOCKS NEW_FILE\n  resume CHECKPOINT NEW_OUTPUT_DIR\n  serve [PORT]\n  benchmark [BLOCKS]"
        ),
        _ => return Err("unknown command; use --help".into()),
    }
    Ok(())
}
