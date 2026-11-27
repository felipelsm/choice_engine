{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {nixpkgs, ...}: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    runtimeLibs = with pkgs; [
      xorg.libX11
      xorg.libXcursor
      xorg.libXrandr
      xorg.libXi
      libxkbcommon
      wayland
    ];
  in {
    devShells.${system}.default = pkgs.mkShell {
      packages = [pkgs.pkg-config];
      LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
    };
  };
}
