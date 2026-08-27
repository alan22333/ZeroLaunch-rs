use crate::core::bias_rule::BiasRule;
use std::collections::HashMap;
use std::sync::Arc;
use zerolaunch_plugin_api::config::Configurable;
use zerolaunch_plugin_api::{
    CachedCandidateData, DataSource, KeywordInjector, KeywordOptimizer, SearchCandidate,
};

pub struct CandidatePipeline {
    data_sources: Vec<Arc<dyn DataSource>>,
    keyword_optimizers: Vec<Arc<dyn KeywordOptimizer>>,
    keyword_injectors: Vec<Arc<dyn KeywordInjector>>,
    bias_rules: HashMap<String, f64>,
}

impl CandidatePipeline {
    pub fn new() -> Self {
        Self {
            data_sources: Vec::new(),
            keyword_optimizers: Vec::new(),
            keyword_injectors: Vec::new(),
            bias_rules: HashMap::new(),
        }
    }

    /// 设置固定偏移量规则列表，内部转换为 HashMap 以支持 O(1) 查找。
    /// 规则按 target 精确匹配（target 已预归一化为 lowercase）。
    pub fn set_bias_rules(&mut self, rules: Vec<BiasRule>) {
        self.bias_rules = rules.into_iter().map(|r| (r.target, r.bias)).collect();
    }

    pub fn add_source(&mut self, source: Arc<dyn DataSource>) {
        self.data_sources.push(source);
    }

    pub fn remove_source(&mut self, component_id: &str) {
        self.data_sources
            .retain(|s| s.component_id() != component_id);
    }

    pub fn add_keyword_optimizer(&mut self, optimizer: Arc<dyn KeywordOptimizer>) {
        self.keyword_optimizers.push(optimizer);
    }

    pub fn remove_keyword_optimizer(&mut self, component_id: &str) {
        self.keyword_optimizers
            .retain(|op| op.component_id() != component_id);
    }

    pub fn add_keyword_injector(&mut self, injector: Arc<dyn KeywordInjector>) {
        self.keyword_injectors.push(injector);
    }

    pub fn remove_keyword_injector(&mut self, component_id: &str) {
        self.keyword_injectors
            .retain(|inj| inj.component_id() != component_id);
    }

    /// 收集数据源候选项并统一过关键字处理流水线（优化器只排序一次、候选只遍历一次）。
    /// 沉浸式插件候选不在此收集，由调用方经 CachedCandidateData::add_plugin_candidate
    /// 单独并入缓存。
    pub async fn collect(&self) -> CachedCandidateData {
        let mut raw: Vec<SearchCandidate> = Vec::new();
        for source in &self.data_sources {
            raw.extend(
                source
                    .fetch_candidates()
                    .await
                    .get_candidates()
                    .iter()
                    .cloned(),
            );
        }

        // 优化器按 priority 升序（一次构建，供全部候选复用）
        let mut sorted: Vec<&dyn KeywordOptimizer> =
            self.keyword_optimizers.iter().map(|a| a.as_ref()).collect();
        sorted.sort_by_key(|op| op.get_priority());

        // 注入器无需排序
        let injectors: Vec<&dyn KeywordInjector> =
            self.keyword_injectors.iter().map(|a| a.as_ref()).collect();

        let mut processed = Vec::with_capacity(raw.len());
        for c in raw {
            processed.push(self.process_candidate(c, &sorted, &injectors).await);
        }

        // 统一去重 + 分配 id（重建索引）
        let mut candidates = CachedCandidateData::new();
        for c in processed {
            candidates.add_candidate(c);
        }
        candidates
    }

    /// 对单个候选运行完整关键字处理流水线（纯函数，值进值出）：
    /// 保留候选自带 keywords → 名称派生（优化器链）→ 注入器 → 去重 → 固定偏置。
    /// `sorted` / `injectors` 由调用方一次性构建传入。
    pub async fn process_candidate(
        &self,
        mut candidate: SearchCandidate,
        sorted: &[&dyn KeywordOptimizer],
        injectors: &[&dyn KeywordInjector],
    ) -> SearchCandidate {
        let mut keywords = std::mem::take(&mut candidate.keywords);
        keywords.extend(Self::apply_keyword_optimizers(&candidate.name, sorted).await);
        for injector in injectors {
            keywords.extend(injector.inject_keywords(&candidate).await);
        }
        candidate.keywords = Self::deduplicate_keywords(keywords);
        let target = candidate.target.payload().to_ascii_lowercase();
        if let Some(bias) = self.bias_rules.get(&target) {
            candidate.bias += bias;
        }
        candidate
    }

    /// 对单个名称运行优化器链，返回去重后的关键字列表。
    /// 参数 `sorted` 必须已按 `get_priority()` 升序排列。
    async fn apply_keyword_optimizers(name: &str, sorted: &[&dyn KeywordOptimizer]) -> Vec<String> {
        let mut accumulated: Vec<String> = vec![name.to_string()];
        for optimizer in sorted {
            let new_keywords = if optimizer.uses_context() {
                let mut out = Vec::new();
                for kw in &accumulated {
                    out.extend(optimizer.optimize(kw).await);
                }
                out
            } else {
                optimizer.optimize(name).await
            };
            accumulated.extend(new_keywords);
        }
        Self::deduplicate_keywords(accumulated)
    }

    fn deduplicate_keywords(keywords: Vec<String>) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        keywords
            .into_iter()
            .filter(|k| seen.insert(k.clone()))
            .collect()
    }

    /// 调试用：对单个名称运行关键字优化器链，返回所有生成的关键字。
    /// 不修改候选项缓存。内部自行排序后调用共享逻辑。
    pub async fn generate_keywords_for_name(&self, name: &str) -> Vec<String> {
        let mut sorted: Vec<&dyn KeywordOptimizer> =
            self.keyword_optimizers.iter().map(|a| a.as_ref()).collect();
        sorted.sort_by_key(|op| op.get_priority());
        Self::apply_keyword_optimizers(name, &sorted).await
    }

    /// 根据 component_id 查找已注册的 Configurable 组件。
    /// 参数：component_id - 组件标识符。
    /// 返回：找到则返回组件引用，否则返回 None。
    pub fn find_configurable(&self, component_id: &str) -> Option<Arc<dyn Configurable>> {
        // 先从数据源中查找
        if let Some(found) = self
            .data_sources
            .iter()
            .find(|s| s.component_id() == component_id)
            .map(|s| s.clone() as Arc<dyn Configurable>)
        {
            return Some(found);
        }
        // 再从关键词优化器中查找
        if let Some(found) = self
            .keyword_optimizers
            .iter()
            .find(|op| op.component_id() == component_id)
            .map(|op| op.clone() as Arc<dyn Configurable>)
        {
            return Some(found);
        }
        // 最后从关键词注入器中查找
        self.keyword_injectors
            .iter()
            .find(|inj| inj.component_id() == component_id)
            .map(|inj| inj.clone() as Arc<dyn Configurable>)
    }
}

impl Default for CandidatePipeline {
    fn default() -> Self {
        Self::new()
    }
}
