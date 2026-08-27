//! 搜索流水线 RPC 载荷全量传入/传出的序列化开销测量。
//!
//! 测量对象：calculate_scores / booster_boost 每次查询携带的完整候选集
//! （宿主 → 插件：请求方向；插件 → 宿主：ScoredCandidate 响应方向）。
//!
//! 运行：cargo run --release --example perf_payload -p zerolaunch-plugin-protocol

use std::time::Instant;
use zerolaunch_plugin_api::services::IconRequest;
use zerolaunch_plugin_api::{
    CachedCandidateData, ExecutionTarget, ScoreDetail, ScoreDetailKind, ScoredCandidate,
    SearchCandidate,
};
use zerolaunch_plugin_protocol::messages::{BoosterBoostParams, CalculateScoresParams};

/// 模拟一个典型数据源候选项（文件名/应用名 + 图标路径 + 执行目标）。
fn make_candidate(i: u64) -> SearchCandidate {
    SearchCandidate {
        id: i,
        name: format!("示例应用/文件名称{:04}.exe", i),
        icon: IconRequest::Path(format!(
            "C:\\Program Files\\ZeroLaunch\\apps\\item{}.exe",
            i
        )),
        target: ExecutionTarget::Path(format!(
            "C:\\Program Files\\ZeroLaunch\\apps\\item{}.exe",
            i
        )),
        keywords: vec!["keyword".into(), format!("kw{}", i % 100)],
        bias: 0.5,
        trigger_keywords: Vec::new(),
    }
}

/// 模拟引擎打分结果（含 2 条分数明细）。
fn make_scored(i: u64) -> ScoredCandidate {
    ScoredCandidate {
        candidate_id: i,
        score: 3.5,
        detailed_score: vec![
            ScoreDetail {
                score: 2.0,
                weight: 1.0,
                description: "名称相似度".into(),
                kind: ScoreDetailKind::Add,
            },
            ScoreDetail {
                score: 1.5,
                weight: 1.0,
                description: "关键词命中".into(),
                kind: ScoreDetailKind::Add,
            },
        ],
    }
}

fn fmt_us(d: std::time::Duration) -> String {
    format!("{:.0}µs", d.as_secs_f64() * 1e6)
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stdout)
        .init();
    for n in [500usize, 2000, 10000] {
        // 模拟宿主侧候选缓存（add_candidate 顺序分配 id）
        let candidates: Vec<SearchCandidate> = (1..=n as u64).map(make_candidate).collect();
        let mut cache = CachedCandidateData::new();
        for c in candidates {
            cache.add_candidate(c);
        }

        // 宿主侧：构造参数快照（to_data）
        let t = Instant::now();
        let params = CalculateScoresParams {
            component_id: "com.example.engine".into(),
            candidates: cache.to_data(),
            query: "示例查询词".into(),
        };
        let to_data = t.elapsed();

        // 宿主侧：JSON 序列化（stdout 传输方向）
        let t = Instant::now();
        let payload = serde_json::to_vec(&params).unwrap();
        let req_ser = t.elapsed();

        // 插件侧：反序列化 + from_data 还原
        let t = Instant::now();
        let back: CalculateScoresParams = serde_json::from_slice(&payload).unwrap();
        let req_de = t.elapsed();
        let t = Instant::now();
        let plugin_cache = CachedCandidateData::from_data(back.candidates);
        let from_data = t.elapsed();
        assert_eq!(plugin_cache.get_candidates().len(), n);

        // 插件侧：响应方向（裸 Vec<ScoredCandidate>）
        let scored: Vec<ScoredCandidate> = (1..=n as u64).map(make_scored).collect();
        let t = Instant::now();
        let resp = serde_json::to_vec(&scored).unwrap();
        let resp_ser = t.elapsed();

        // booster_boost 双载（candidates + scored 同时传输）
        let t = Instant::now();
        let boost_params = BoosterBoostParams {
            component_id: "com.example.booster".into(),
            candidates: cache.to_data(),
            scored: scored.clone(),
            query: "示例查询词".into(),
        };
        let boost_payload = serde_json::to_vec(&boost_params).unwrap();
        let boost_ser = t.elapsed();

        tracing::info!(
            n = n,
            req_bytes = payload.len(),
            resp_bytes = resp.len(),
            boost_bytes = boost_payload.len(),
            to_data_us = format_args!("{}", fmt_us(to_data)),
            req_ser_us = format_args!("{}", fmt_us(req_ser)),
            req_de_us = format_args!("{}", fmt_us(req_de)),
            from_data_us = format_args!("{}", fmt_us(from_data)),
            resp_ser_us = format_args!("{}", fmt_us(resp_ser)),
            boost_ser_us = format_args!("{}", fmt_us(boost_ser)),
            "payload perf"
        );
    }
}
