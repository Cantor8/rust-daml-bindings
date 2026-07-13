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

        # Daml SDK 3.4.x produces LF 2.x DARs, which is what the
        # LF2-only `daml-lf` crate consumes. Paired with Canton
        # 3.5.x for the participant side — the SDK toolchain and
        # the participant binary track different release trains;
        # the minor mismatch is fine because `daml build` only
        # depends on the LF specification, not the participant.
        damlSdkVersion = "3.4.11";

        damlSdkSrc = pkgs.fetchurl {
          url = "https://github.com/digital-asset/daml/releases/download/v${damlSdkVersion}/daml-sdk-${damlSdkVersion}-linux-x86_64.tar.gz";
          sha256 = "0bf6l6drkblzrdh4yf47c0c745k593jji9lm4nxh29d6mjin4xb0";
        };

        # Daml SDK — exposes `daml build` (and the rest of the
        # Daml Assistant CLI) for compiling LF2 DARs locally. The
        # SDK ships a self-contained tree under
        # $out/share/daml-sdk/<version>; the wrapper script keeps
        # DAML_SDK / DAML_HOME pointing into that tree so the
        # toolchain doesn't try to write to `~/.daml`.
        damlSdk = pkgs.stdenv.mkDerivation {
          pname = "daml-sdk";
          version = damlSdkVersion;
          src = damlSdkSrc;
          nativeBuildInputs = [ pkgs.makeWrapper pkgs.autoPatchelfHook ];
          # The bundled JDK / Haskell binaries link against glibc +
          # libstdc++ + zlib + ncurses; autoPatchelfHook needs
          # these in scope to rewrite their rpaths.
          buildInputs = [ pkgs.stdenv.cc.cc.lib pkgs.zlib pkgs.ncurses5 ];
          # The SDK includes prebuilt Linux ELF binaries; skip the
          # cross-arch and broken-symlink scans that would
          # otherwise scrub them.
          dontStrip = true;
          dontPatchELF = false;
          installPhase = ''
            sdkdir="$out/share/daml-sdk/${damlSdkVersion}"
            mkdir -p "$sdkdir" "$out/bin"
            cp -r . "$sdkdir/"
            makeWrapper "$sdkdir/daml/daml" "$out/bin/daml" \
              --set JAVA_HOME ${pkgs.jdk21}/lib/openjdk \
              --set DAML_SDK "$sdkdir" \
              --set DAML_SDK_VERSION "${damlSdkVersion}" \
              --prefix PATH : ${pkgs.jdk21}/bin
          '';
          meta = {
            description = "Daml SDK — daml build, daml-assistant, daml-libs";
            homepage = "https://daml.com/";
            license = pkgs.lib.licenses.asl20;
            platforms = [ "x86_64-linux" ];
          };
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
          inherit canton canton-sandbox damlSdk;
          default = canton-sandbox;
        };

        devShells = let
          # Rust build surface shared by CI and local dev: the toolchain
          # plus the native deps prost-build / tonic-build and transitive
          # crates need. Everything here resolves from cache.nixos.org, so
          # entering this shell on a cold runner is a fast binary fetch
          # rather than a source build.
          rustPackages = with pkgs; [
            rustToolchain
            rustfmtNightly

            # Build deps for tonic-build / prost-build (daml-grpc, daml-lf).
            protobuf

            # Native deps commonly needed by transitive crates.
            pkg-config
            openssl

            # Mirrors the cargo-deny CI job.
            cargo-deny
          ];

          rustEnv = {
            # Help tonic-build / prost-build find protoc deterministically.
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            PROTOC_INCLUDE = "${pkgs.protobuf}/include";

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };
        in {
          # Lightweight shell for the check / test / clippy / fmt /
          # cargo-deny CI jobs (`nix develop .#ci`). Deliberately EXCLUDES
          # the JDK, Canton and the Daml SDK: those are custom
          # (non-cache.nixos.org) derivations whose ~1 GB closure would be
          # built from scratch on every cold runner. Only the integration
          # job's live sandbox needs them, so keeping them out of this
          # shell keeps the bulk of CI a quick binary-cache fetch.
          ci = pkgs.mkShell {
            packages = rustPackages;
            env = rustEnv;
          };

          # Full local + integration environment: the Rust surface above
          # plus a local Canton sandbox, the Daml SDK, and dev-only cargo
          # tooling. Used by the integration CI job and for daily work.
          default = pkgs.mkShell {
            packages = rustPackages ++ (with pkgs; [
              # Cargo tooling — quality-of-life helpers for local dev.
              cargo-edit
              cargo-outdated
              cargo-audit
              cargo-nextest

              # Local Canton sandbox for integration tests.
              jdk21
              canton
              canton-sandbox

              # Daml SDK for building LF2 .dar fixtures from .daml sources.
              damlSdk

              # Handy for poking the gRPC surface from the shell.
              grpcurl
            ]);

            env = rustEnv // {
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
              echo "  daml SDK:        ${damlSdk}/share/daml-sdk/${damlSdkVersion} (v${damlSdkVersion})"
              echo ""
              echo "  Start the sandbox with: canton-sandbox"
              echo "  Ledger API listens on:  localhost:5011"
              echo "  Admin API  listens on:  localhost:5012"
              echo "  Build a DAR with:       daml build (in a project dir with daml.yaml)"
            '';
          };
        };
      });
}
