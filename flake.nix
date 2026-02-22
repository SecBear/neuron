{
  description = "neuron — composable building blocks for AI agents";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        {
          inputs',
          pkgs,
          system,
          ...
        }:
        let
          rustToolchain = inputs'.fenix.packages.stable.withComponents [
            "cargo"
            "clippy"
            "rust-src"
            "rustc"
            "rustfmt"
            "rust-analyzer"
          ];
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.fenix.overlays.default ];
          };

          devShells.default = pkgs.mkShell {
            buildInputs =
              [
                rustToolchain
                pkgs.cargo-watch
                pkgs.cargo-edit

                # reqwest (HTTP client used by all provider crates)
                pkgs.pkg-config
                pkgs.openssl

                # Nix tooling
                pkgs.nixd
                pkgs.nixfmt
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
                pkgs.libiconv
              ];

            OPENSSL_NO_VENDOR = 1;
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

            shellHook = ''
              echo "neuron dev shell"
              echo ""
              echo "  rustc --version   — $(rustc --version)"
              echo "  cargo clippy      — lint all crates"
              echo "  cargo watch       — build with hot reload"
              echo ""
            '';
          };
        };
    };
}
