{
  description = "Clepho - TUI photo management application";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain
            rustToolchain
            cargo-watch
            cargo-edit

            # Native dependencies
            pkg-config
            openssl
            onnxruntime
          ];

          shellHook = ''
            echo "Clepho development environment"
            echo "Run 'cargo build' to compile"
            echo "Run 'cargo run' to start the application"
          '';

          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          ORT_DYLIB_PATH = "${pkgs.onnxruntime}/lib/libonnxruntime.so";
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "clepho";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
            makeWrapper
          ];

          buildInputs = with pkgs; [
            openssl
            onnxruntime
          ];

          # Point ort to the onnxruntime library at build time
          ORT_DYLIB_PATH = "${pkgs.onnxruntime}/lib/libonnxruntime.so";

          # Wrap the binary to include onnxruntime path at runtime
          postInstall = ''
            wrapProgram $out/bin/clepho \
              --set ORT_DYLIB_PATH "${pkgs.onnxruntime}/lib/libonnxruntime.so"
            wrapProgram $out/bin/clepho-daemon \
              --set ORT_DYLIB_PATH "${pkgs.onnxruntime}/lib/libonnxruntime.so"
          '';
        };
      }
    );
}
