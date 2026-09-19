use anyhow::{Result, bail};

use crate::backend::Backends;
use crate::cli::StatArgs;

pub async fn run(_backends: &mut Backends, _args: StatArgs) -> Result<()> {
    bail!("stat is not implemented yet")
}
