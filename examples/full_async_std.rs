use vfs_nodes::AsyncStdFileSystemScheme;

mod full;

#[async_std::main]
async fn main() -> anyhow::Result<()> {
	// We pass in a function to create the async-std set of schemes
	full::run_vfs_examples(|path| Box::new(AsyncStdFileSystemScheme::new(path))).await
}
