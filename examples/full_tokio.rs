use vfs_nodes::TokioFileSystemScheme;

mod full;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	// We pass in a function to create the async-std set of schemes
	full::run_vfs_examples(|path| Box::new(TokioFileSystemScheme::new(path))).await
}
