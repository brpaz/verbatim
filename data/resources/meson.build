# Blueprint UI files, compiled to GtkBuilder XML in the build dir
blueprints = custom_target(
  'blueprints',
  input: files('ui/preferences.blp', 'ui/shortcuts.blp', 'ui/window.blp'),
  output: '.',
  command: [
    find_program('blueprint-compiler'),
    'batch-compile',
    '@OUTPUT@',
    '@CURRENT_SOURCE_DIR@',
    '@INPUT@',
  ],
)

# Resources
resources = gnome.compile_resources(
  'resources',
  'resources.gresource.xml',
  dependencies: blueprints,
  gresource_bundle: true,
  source_dir: meson.current_build_dir(),
  install: true,
  install_dir: pkgdatadir,
)
