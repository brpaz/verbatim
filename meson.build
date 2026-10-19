project(
  'verbatim',
  'rust',
  version: files('VERSION'),
  meson_version: '>= 0.59',
  license: 'MIT',
)

i18n = import('i18n')
gnome = import('gnome')

base_id = 'dev.brunopaz.Verbatim'

dependency('glib-2.0', version: '>= 2.66')
dependency('gio-2.0', version: '>= 2.66')
dependency('gtk4', version: '>= 4.20')
dependency('gstreamer-1.0', version: '>= 1.20')
dependency('libadwaita-1', version: '>= 1.8')

find_program('glib-compile-resources')
find_program('blueprint-compiler')
glib_compile_schemas = find_program('glib-compile-schemas')
desktop_file_validate = find_program('desktop-file-validate', required: false)
appstreamcli = find_program('appstreamcli', required: false)
cargo = find_program('cargo')

version = meson.project_version()

prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
localedir = prefix / get_option('localedir')

datadir = prefix / get_option('datadir')
pkgdatadir = datadir / meson.project_name()
iconsdir = datadir / 'icons'
podir = meson.project_source_root() / 'po'
gettext_package = meson.project_name()

if get_option('profile') == 'development'
  message('Building in debug mode')

  profile = 'Devel'
  vcs_tag = run_command('git', 'rev-parse', '--short', 'HEAD', check: false).stdout().strip()
  if vcs_tag == ''
    version_suffix = '-devel'
  else
    version_suffix = '-@0@'.format(vcs_tag)
  endif
  application_id = '@0@.@1@'.format(base_id, profile)

  cargo_release_option = []
  rust_target = 'debug'
else
  message('Building in release mode')

  profile = ''
  version_suffix = ''
  application_id = base_id
  cargo_release_option = ['--release']
  rust_target = 'release'
endif

cargo_home = meson.project_build_root() / 'cargo-home'
cargo_target = meson.project_build_root() / 'cargo-target'
cargo_env = environment()
cargo_env.set('CARGO_HOME', cargo_home)
cargo_env.set('CARGO_TARGET_DIR', cargo_target)
cargo_env.set('MESON_APP_ID', application_id)
cargo_env.set('MESON_GETTEXT_PACKAGE', gettext_package)
cargo_env.set('MESON_LOCALEDIR', localedir)
cargo_env.set('MESON_PKGDATADIR', pkgdatadir)
cargo_env.set('MESON_PROFILE', profile)
cargo_env.set('MESON_VERSION', version + version_suffix)
cargo_env.set('MESON_RESOURCES_FILE', pkgdatadir / 'resources.gresource')

devenv_env = environment()
devenv_env.set('MESON_APP_ID', application_id)
devenv_env.set('MESON_GETTEXT_PACKAGE', gettext_package)
devenv_env.set('MESON_LOCALEDIR', meson.project_build_root() / 'po')
devenv_env.set('MESON_PKGDATADIR', meson.project_build_root() / 'data')
devenv_env.set('GSETTINGS_SCHEMA_DIR', meson.project_build_root() / 'data')
devenv_env.set('MESON_PROFILE', profile)
devenv_env.set('MESON_VERSION', version + version_suffix)
devenv_env.set(
  'MESON_RESOURCES_FILE',
  meson.project_build_root() / 'data' / 'resources' / 'resources.gresource',
)

meson.add_devenv(devenv_env)

# Required if you run dist though
find_program('cargo-vendor-filterer', required: false)
meson.add_dist_script(
  'build-aux/dist-vendor.sh',
  cargo_home,
  cargo_target,
)

subdir('data')
subdir('po')
subdir('src')

# Unit tests read the bundle from the build tree, not the install prefix.
test_env = environment()
test_env.set('CARGO_HOME', cargo_home)
test_env.set('CARGO_TARGET_DIR', cargo_target)
test_env.set('MESON_APP_ID', application_id)
test_env.set('MESON_GETTEXT_PACKAGE', gettext_package)
test_env.set('MESON_LOCALEDIR', localedir)
test_env.set('MESON_PKGDATADIR', pkgdatadir)
test_env.set('MESON_PROFILE', profile)
test_env.set('MESON_VERSION', version + version_suffix)
test_env.set(
  'MESON_RESOURCES_FILE',
  meson.project_build_root() / 'data' / 'resources' / 'resources.gresource',
)

test(
  'cargo-test',
  cargo,
  args: [
    'test',
    cargo_release_option,
    '--all-targets',
    '--all-features',
    '--workspace',
  ],
  env: test_env,
  depends: resources,
  timeout: 400,
)

gnome.post_install(
  gtk_update_icon_cache: true,
  glib_compile_schemas: true,
  update_desktop_database: true,
)
