use jacquard_lexicon::{codegen::CodeGenerator, corpus::LexiconCorpus};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let corpus = LexiconCorpus::load_from_dir("./lexicons")?;
    let codegen = CodeGenerator::new(&corpus, "crate");
    codegen.write_to_disk(Path::new("./src/lexicons"))?;

    Ok(())
}
