#!/bin/sh
case "${1-}" in
  ''|help|-h|--help|-V|--version|init|templates|plugin|export|info|doctor)
    exec /opt/celesta/celesta "$@"
    ;;
  *)
    exec /opt/celesta/celesta-exporter "$@"
    ;;
esac
