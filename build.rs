use embed_manifest::{
	embed_manifest,
	manifest::{ActiveCodePage, Setting, SupportedOS::*},
	new_manifest,
};

fn main() {
	// Check if we're building for Windows (either natively or cross-compiling)
	let target = std::env::var("TARGET").unwrap_or_default();
	if target.contains("windows") {
		// Create a comprehensive manifest for Windows theming and modern features
		let manifest = new_manifest("Tubex") // Use app name "Tubex"
			// Enable modern Windows Common Controls (v6) for theming
			.supported_os(Windows7..=Windows10)
			// Set UTF-8 as active code page for better Unicode support
			.active_code_page(ActiveCodePage::Utf8)
			// Enable heap type optimization for better performance (if available)
			.heap_type(embed_manifest::manifest::HeapType::SegmentHeap)
			// Enable high-DPI awareness for crisp displays
			.dpi_awareness(embed_manifest::manifest::DpiAwareness::PerMonitorV2)
			// Enable long path support (if configured in Windows)
			.long_path_aware(Setting::Enabled);

		// Embed the manifest - this works even when cross-compiling!
		if let Err(e) = embed_manifest(manifest) {
			println!("cargo:warning=Failed to embed manifest: {}", e);
			println!("cargo:warning=The application will still work but may lack optimal Windows theming");
		}
		// Tell Cargo to rerun this build script if the build script changes
		println!("cargo:rerun-if-changed=build.rs");
	}
}
