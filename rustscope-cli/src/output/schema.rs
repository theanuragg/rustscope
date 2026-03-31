pub use rustscope::output::schema::{
    MemoryEvent,
    ProcessSample as Sample,
    ProcessSummary as Summary,
    ProfileSession as OutputSchema,
    RollupRecord as Rollup,
};

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub module: String,
    pub self_pct: f64,
    pub total_pct: f64,
    pub calls: u64,
    pub avg_ns: u64,
    pub depth: u32,
    pub x: f64,
    pub w: f64,
}
