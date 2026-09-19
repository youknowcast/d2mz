use anyhow::{Result, bail};

use crate::backend::Backends;
use crate::cli::CatArgs;

pub async fn run(_backends: &mut Backends, _args: CatArgs) -> Result<()> {
    bail!("cat is not implemented yet")
}
