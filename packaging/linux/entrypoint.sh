#!/bin/sh
if [ ! -d "${HOME:-}" ] || [ ! -w "$HOME" ]; then
  case "${XDG_CONFIG_HOME:-}" in
    /*) ;;
    *)
      case "${HOME:-}" in
        /*) if [ -d "$HOME/.config" ]; then export XDG_CONFIG_HOME="$HOME/.config"; fi ;;
      esac
      ;;
  esac
  HOME="$(mktemp -d /tmp/celesta-home.XXXXXX)" || exit 1
  export HOME
fi

case "${1-}" in
  ''|help|-h|--help|-V|--version|init|templates|plugin|export|info|doctor)
    exec /opt/celesta/celesta "$@"
    ;;
  *)
    exec /opt/celesta/celesta-exporter "$@"
    ;;
esac
