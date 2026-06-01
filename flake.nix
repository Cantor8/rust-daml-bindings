{
  description = "Development environment for rust-daml-bindings";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # Stable toolchain with the components needed for daily work.
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "llvm-tools-preview" ];
        };

        # rustfmt.toml sets `unstable_features = true`, which requires nightly
        # rustfmt. Expose it alongside the stable toolchain so `cargo fmt`
        # workflows can pick it up without replacing the primary rustc.
        rustfmtNightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain:
          toolchain.minimal.override { extensions = [ "rustfmt" ]; });

        cantonVersion = "3.5.1";

        cantonSrc = pkgs.fetchurl {
          url = "https://github.com/digital-asset/canton/releases/download/v${cantonVersion}/canton-open-source-${cantonVersion}.tar.gz";
          sha256 = "0gdjrhiqa2djjam42cdw5m80fcdbq2fcy7bk3jyqv8z9cmmycwby";
        };

        # Canton open-source distribution. Wraps the upstream `bin/canton`
        # launcher so JAVA_HOME points at a managed JDK.
        canton = pkgs.stdenv.mkDerivation {
          pname = "canton";
          version = cantonVersion;
          src = cantonSrc;
          nativeBuildInputs = [ pkgs.makeWrapper ];
          installPhase = ''
            mkdir -p $out/share/canton $out/bin
            cp -r . $out/share/canton/
            makeWrapper $out/share/canton/bin/canton $out/bin/canton \
              --set JAVA_HOME ${pkgs.jdk21}/lib/openjdk \
              --prefix PATH : ${pkgs.jdk21}/bin
          '';
          meta = {
            description = "Canton open-source — Daml synchronizer + participant";
            homepage = "https://www.canton.io/";
            license = pkgs.lib.licenses.asl20;
          };
        };

        # Convenience wrapper that boots the in-memory sandbox defined in
        # rust-daml-bindings/canton/{sandbox.conf,sandbox.canton}.
        canton-sandbox = pkgs.writeShellScriptBin "canton-sandbox" ''
          set -euo pipefail
          # Locate the rust-daml-bindings checkout. Allow override via
          # RUST_DAML_BINDINGS_DIR; otherwise probe upwards from the cwd.
          if [ -n "''${RUST_DAML_BINDINGS_DIR:-}" ]; then
            repo="$RUST_DAML_BINDINGS_DIR"
          else
            d="$PWD"
            while [ "$d" != "/" ]; do
              if [ -f "$d/canton/sandbox.conf" ] && [ -f "$d/canton/sandbox.canton" ]; then
                repo="$d"
                break
              fi
              d=$(dirname "$d")
            done
          fi
          if [ -z "''${repo:-}" ]; then
            echo "canton-sandbox: could not find canton/sandbox.conf — run from inside the rust-daml-bindings checkout, or set RUST_DAML_BINDINGS_DIR." >&2
            exit 1
          fi
          echo "canton-sandbox: using config from $repo/canton" >&2
          exec ${canton}/bin/canton daemon \
            -c "$repo/canton/sandbox.conf" \
            --bootstrap "$repo/canton/sandbox.canton" \
            "$@"
        '';
      in
      {
        packages = {
          inherit canton canton-sandbox;
          default = canton-sandbox;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            rustfmtNightly

            # Build deps for tonic-build / prost-build (daml-grpc, daml-lf).
            protobuf

            # Native deps commonly needed by transitive crates.
            pkg-config
            openssl

            # Cargo tooling — cargo-deny mirrors the CI job; the rest are
            # quality-of-life helpers for local development.
            cargo-deny
            cargo-edit
            cargo-outdated
            cargo-audit
            cargo-nextest

            # Local Canton sandbox for integration tests.
            jdk21
            canton
            canton-sandbox

            # Handy for poking the gRPC surface from the shell.
            grpcurl
          ];

          env = {
            # Help tonic-build / prost-build find protoc deterministically.
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            PROTOC_INCLUDE = "${pkgs.protobuf}/include";

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

            # Used by Canton's launcher.
            JAVA_HOME = "${pkgs.jdk21}/lib/openjdk";
          };

          shellHook = ''
            echo "rust-daml-bindings dev shell"
            echo "  rustc:           $(rustc --version)"
            echo "  cargo:           $(cargo --version)"
            echo "  protoc:          $(protoc --version)"
            echo "  java:            $(java -version 2>&1 | head -1)"
            echo "  canton:          ${canton}/share/canton (v${cantonVersion})"
            echo ""
            echo "  Start the sandbox with: canton-sandbox"
            echo "  Ledger API listens on:  localhost:5011"
            echo "  Admin API  listens on:  localhost:5012"
          '';
        };
      });
}
