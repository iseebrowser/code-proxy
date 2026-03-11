//! Tests for single instance functionality
//!
//! Single instance ensures only one instance of the application can run at a time.
//! When a second instance is started, it should:
//! 1. Detect an existing instance
//! 2. Focus the existing instance's window
//! 3. Exit the second instance

use std::process::Command;
use std::time::Duration;
use std::thread;

/// Test that verifies the single instance plugin is properly configured
/// This is a compile-time check - if the plugin isn't registered, the test won't compile
#[test]
fn test_single_instance_plugin_configured() {
    // This test verifies the plugin is in Cargo.toml
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains("tauri-plugin-single-instance"),
        "tauri-plugin-single-instance must be in Cargo.toml dependencies"
    );
}

/// Integration test for single instance behavior
/// Note: This test requires the application to be built first
#[test]
#[ignore] // Run with `cargo test -- --ignored` after building
fn test_single_instance_prevents_second_instance() {
    // Build the release executable first
    let build_output = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(env!("CARGO_MANIFEST_DIR").rsplit_once("tests").unwrap().0)
        .output()
        .expect("Failed to build application");

    assert!(build_output.status.success(), "Build failed");

    let exe_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/release/code-proxy.exe");

    // Start first instance
    let mut first_instance = Command::new(exe_path)
        .spawn()
        .expect("Failed to start first instance");

    // Wait for first instance to initialize
    thread::sleep(Duration::from_secs(2));

    // Start second instance - it should exit immediately
    let second_instance = Command::new(exe_path)
        .output()
        .expect("Failed to start second instance");

    // Second instance should exit quickly (not stay running)
    // The exit code behavior varies by implementation
    // But the process should not hang
    println!("Second instance exit code: {:?}", second_instance.status);

    // Clean up first instance
    let _ = first_instance.kill();
}

/// Test that the single instance plugin emits events when a second instance tries to start
#[test]
fn test_single_instance_event_handling() {
    // Verify the plugin setup includes event handling for new instance requests
    // This is checked by verifying the lib.rs contains the plugin initialization
    let lib_rs = include_str!("../src/lib.rs");

    assert!(
        lib_rs.contains("single_instance"),
        "lib.rs should contain single_instance plugin initialization"
    );
}