{
  description = "Trusted Autonomy — local-first agent substrate";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Read toolchain from rust-toolchain.toml so Nix and rustup stay in sync
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Native build inputs needed for compilation (C linker, pkg-config)
        nativeBuildInputs = with pkgs; [
          pkg-config
        ];

        # Libraries needed at build and runtime
        buildInputs = with pkgs; [
          openssl
        ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.libiconv
        ];

      in {
        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;

          packages = with pkgs; [
            # Rust toolchain (compiler, cargo, rustfmt, clippy, rust-analyzer)
            rustToolchain

            # Version control
            git

            # Dev tools
            cargo-nextest  # faster test runner with better output
            just           # command runner (like make, but simpler)
          ];

          shellHook = ''
            echo "Trusted Autonomy dev environment loaded"
            echo "  Rust: $(rustc --version)"
            echo "  Cargo: $(cargo --version)"
            echo ""
            echo "Quick start:"
            echo "  just build   — build all crates"
            echo "  just test    — run all tests"
            echo "  just check   — lint + format check"
          '';
        };
      }
    );
}
