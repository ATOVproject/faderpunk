{pkgs, ...}: {
  dotenv.enable = false;

  packages =
    [pkgs.pkg-config]
    ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
      pkgs.alsa-lib
      pkgs.wayland
      pkgs.libxkbcommon
      pkgs.vulkan-loader
      pkgs.libGL
    ];

  env = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
    LD_LIBRARY_PATH =
      pkgs.lib.makeLibraryPath [
        pkgs.wayland
        pkgs.libxkbcommon
        pkgs.vulkan-loader
        pkgs.libGL
      ]
      + ":/run/opengl-driver/lib";
  };
}
