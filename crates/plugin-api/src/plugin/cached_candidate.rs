use crate::plugin::types::{CandidateId, ExecutionTarget, SearchCandidate};
use dashmap::DashMap;
use dashmap::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct CachedCandidateData {
    /// 当前缓存的候选数据
    candidates: Vec<SearchCandidate>,
    /// 候选ID到索引的映射
    index: DashMap<CandidateId, usize>,
    /// 该方法用于去重，只有没有重复的候选项才会被添加到candidates中，重复的候选项会被丢弃掉
    /// 判断的依据：执行目标
    cached_targets: HashSet<ExecutionTarget>,
    /// 该方法用于去重，只有显示名不重复的候选项才会被添加到candidates中
    /// 判断的依据：候选项显示名（忽略大小写）
    cached_display_names: HashSet<String>,
    /// 下一个候选项ID
    next_candidate_id: CandidateId,
    /// 缓存世代：每次全量重建递增。前端确认时回传该世代，
    /// 后端校验不匹配即拒绝——防止缓存刷新后 id 漂移导致确认到错误候选。
    /// 不随跨 RPC 快照传输（快照仅用于流水线组件打分，不含确认语义）。
    generation: u64,
}

/// CachedCandidateData 的跨 RPC 序列化快照（宿主缓存 → 插件进程）。
///
/// 由 CachedCandidateData::to_data 导出、from_data 还原，仅用于搜索流水线
/// 组件（SearchEngine / ScoreBooster）经 RPC 接收宿主候选缓存的传输；
/// index / 去重集合不随快照传输——它们由候选列表确定性派生（id 按插入
/// 顺序自增分配、无删除），from_data 以 debug_assert 校验后重建。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateCacheSnapshot {
    /// 候选列表（保持 id 自增分配顺序，id 与位置一一对应）。
    #[serde(rename = "candidates")]
    pub candidates: Vec<SearchCandidate>,
    /// 下一个待分配的候选项 ID（导出时刻的 next_candidate_id）。
    #[serde(rename = "nextCandidateId")]
    pub next_candidate_id: CandidateId,
}

impl Default for CachedCandidateData {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedCandidateData {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            index: DashMap::new(),
            cached_targets: HashSet::new(),
            cached_display_names: HashSet::new(),
            next_candidate_id: 1,
            generation: 0,
        }
    }

    /// 当前缓存世代（前端确认回传比对用）。
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// 全量重建完成后递增世代：旧确认载荷（携带旧世代）被 route_confirm 拒绝。
    pub fn bump_generation(&mut self) {
        self.generation += 1;
    }

    /// 导出跨 RPC 序列化快照（与 from_data 一一对应）。
    /// 用于搜索流水线组件（引擎/增强器）经 RPC 接收宿主候选缓存。
    pub fn to_data(&self) -> CandidateCacheSnapshot {
        CandidateCacheSnapshot {
            candidates: self.candidates.clone(),
            next_candidate_id: self.next_candidate_id,
        }
    }

    /// 从跨 RPC 序列化快照还原缓存（保真 id 序列，与 to_data 一一对应）。
    /// 参数：data - to_data() 导出的快照。
    /// debug 构建校验快照不变量：id 与位置一一对应、id 唯一、目标与显示名无重复。
    pub fn from_data(data: CandidateCacheSnapshot) -> Self {
        let index = DashMap::new();
        let mut cached_targets = HashSet::new();
        let mut cached_display_names = HashSet::new();
        for (pos, candidate) in data.candidates.iter().enumerate() {
            debug_assert_eq!(
                candidate.id,
                pos as CandidateId + 1,
                "快照候选 id 与位置不对应: id = {}, pos = {}",
                candidate.id,
                pos
            );
            index.insert(candidate.id, pos);
            debug_assert!(
                cached_targets.insert(candidate.target.clone()),
                "快照候选中存在重复执行目标: {:?}",
                candidate.target
            );
            debug_assert!(
                cached_display_names.insert(candidate.name.to_lowercase()),
                "快照候选中存在重复显示名: {}",
                candidate.name
            );
        }
        debug_assert_eq!(
            data.next_candidate_id,
            data.candidates.len() as CandidateId + 1,
            "快照 next_candidate_id 与候选数量不对应: next = {}, len = {}",
            data.next_candidate_id,
            data.candidates.len()
        );
        Self {
            candidates: data.candidates,
            index,
            cached_targets,
            cached_display_names,
            next_candidate_id: data.next_candidate_id,
            generation: 0,
        }
    }

    /// 添加一个候选人
    pub fn add_candidate(&mut self, mut candidate: SearchCandidate) {
        if self.has_target(&candidate.target) || self.has_display_name(&candidate.name) {
            debug!(
                "候选项已存在，丢弃重复的候选项: target = {:?}, name = {}",
                candidate.target, candidate.name
            );
            return;
        }
        let candidate_id = self.next_candidate_id;
        candidate.id = candidate_id;
        self.cached_targets.insert(candidate.target.clone());
        self.cached_display_names
            .insert(candidate.name.to_lowercase());
        self.candidates.push(candidate);
        self.index.insert(candidate_id, self.candidates.len() - 1);
        self.next_candidate_id += 1;
    }

    /// 添加宿主插件候选（沉浸式插件唤醒项）。
    /// 仅按执行目标去重（target 为 ExecutionTarget::Plugin(id)，注册表保证唯一）；
    /// 不按展示名去重——插件候选与数据源候选可能同名，互不丢弃。
    pub fn add_plugin_candidate(&mut self, mut candidate: SearchCandidate) {
        if self.has_target(&candidate.target) {
            warn!(
                "插件候选项已存在，丢弃重复项: target = {:?}",
                candidate.target
            );
            return;
        }
        let candidate_id: u64 = self.next_candidate_id;
        candidate.id = candidate_id;
        self.cached_targets.insert(candidate.target.clone());
        self.candidates.push(candidate);
        self.index.insert(candidate_id, self.candidates.len() - 1);
        self.next_candidate_id += 1;
    }

    /// 根据id获得指定的一个候选人
    pub fn get_candidate(&self, id: CandidateId) -> Option<&SearchCandidate> {
        match self.index.entry(id) {
            Entry::Occupied(entry) => Some(&self.candidates[*entry.get()]),
            Entry::Vacant(_) => None,
        }
    }

    /// 添加多个候选人
    pub fn add_candidates(&mut self, candidates: CachedCandidateData) {
        for candidate in candidates.candidates.iter() {
            self.add_candidate(candidate.clone());
        }
    }

    /// 获得原始的数据
    pub fn get_candidates(&self) -> &Vec<SearchCandidate> {
        &self.candidates
    }

    /// 获得原始的数据的可变引用
    pub fn get_candidates_mut(&mut self) -> &mut Vec<SearchCandidate> {
        &mut self.candidates
    }

    /// 判断是否已经缓存了某个执行目标的候选项了
    fn has_target(&self, target: &ExecutionTarget) -> bool {
        self.cached_targets.contains(target)
    }

    /// 判断是否已经缓存了某个显示名的候选项（忽略大小写）
    fn has_display_name(&self, display_name: &str) -> bool {
        self.cached_display_names
            .contains(&display_name.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::IconRequest;

    fn sample_candidate(i: u64) -> SearchCandidate {
        SearchCandidate {
            id: i,
            name: format!("候选{}", i),
            icon: IconRequest::Path(format!("C:\\item{}.exe", i)),
            target: ExecutionTarget::Path(format!("C:\\item{}.exe", i)),
            keywords: vec!["kw".into()],
            bias: 0.0,
            trigger_keywords: Vec::new(),
        }
    }

    /// to_data → from_data 一一对应：id 序列、顺序、内容全部保真，
    /// next_candidate_id 还原后继续自增分配。
    #[test]
    fn test_snapshot_roundtrip() {
        let mut cache = CachedCandidateData::new();
        for i in 1..=10u64 {
            cache.add_candidate(sample_candidate(i));
        }
        let mut restored = CachedCandidateData::from_data(cache.to_data());
        assert_eq!(restored.get_candidates().len(), 10);
        for i in 1..=10u64 {
            let c = restored.get_candidate(i).expect("id 保真");
            assert_eq!(c.name, format!("候选{}", i));
            assert_eq!(
                c.target,
                ExecutionTarget::Path(format!("C:\\item{}.exe", i))
            );
        }
        // next_candidate_id 还原为 11：新候选 id 从 11 继续
        restored.add_candidate(sample_candidate(0));
        assert_eq!(restored.get_candidates().len(), 11);
        assert_eq!(
            restored.get_candidate(11).expect("next_id 保真").name,
            "候选0"
        );
    }

    /// 快照经 JSON 序列化后往返，保真性不变（跨 RPC 实际传输路径）。
    #[test]
    fn test_snapshot_json_roundtrip() {
        let mut cache = CachedCandidateData::new();
        for i in 1..=5u64 {
            cache.add_candidate(sample_candidate(i));
        }
        let json = serde_json::to_value(cache.to_data()).unwrap();
        let data: CandidateCacheSnapshot = serde_json::from_value(json).unwrap();
        let restored = CachedCandidateData::from_data(data);
        assert_eq!(restored.get_candidates().len(), 5);
        for i in 1..=5u64 {
            assert_eq!(
                restored.get_candidate(i).expect("id 保真").name,
                format!("候选{}", i)
            );
        }
    }

    /// id 与位置不对应的快照在 debug 构建下立即暴露（release 不校验）。
    #[test]
    #[should_panic(expected = "id 与位置不对应")]
    fn test_from_data_detects_mismatched_id() {
        let data = CandidateCacheSnapshot {
            candidates: vec![sample_candidate(5)],
            next_candidate_id: 2,
        };
        CachedCandidateData::from_data(data);
    }
}
