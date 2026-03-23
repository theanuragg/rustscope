pub mod schema;

use std::fs::File;
use std::io::BufWriter;
use anyhow::Result;
use schema::OutputSchema;

pub fn write_json(path: &str, data: &OutputSchema) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, data)?;
    Ok(())
}
