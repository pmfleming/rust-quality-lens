{
  description = "Development environment for rust-quality-lens";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              cargo-nextest
              cargo-llvm-cov
              llvmPackages.llvm
              jq
              cargo-watch
              cargo-audit
              cargo-deny
            ];

            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            LLVM_COV = "${pkgs.llvmPackages.llvm}/bin/llvm-cov";
            LLVM_PROFDATA = "${pkgs.llvmPackages.llvm}/bin/llvm-profdata";

            shellHook = ''
              echo "rust-quality-lens dev shell"
              rustc --version
              cargo --version
            '';
          };
        });
    };
}
