#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Uso: $0 <archivo.mp3> [carpeta_salida]" >&2
  exit 1
fi

input="$1"
output_dir="${2:-${input%.*}_variants}"

if [[ ! -f "$input" ]]; then
  echo "No existe el archivo de entrada: $input" >&2
  exit 1
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg no está instalado" >&2
  exit 1
fi

mkdir -p "$output_dir"

base_name="$(basename "${input%.*}")"

encode() {
  local ext="$1"
  local ffmpeg_args=("-y" "-i" "$input")
  local target="$output_dir/${base_name}.${ext}"

  case "$ext" in
    mp3|mpa|m1a|m2a)
      ffmpeg_args+=("-c:a" "libmp3lame" "$target")
      ;;
    aac|adts)
      ffmpeg_args+=("-c:a" "aac" "$target")
      ;;
    m4a|m4b|m4p|m4r|mp4|3gp|3g2|mov|qt|alac)
      ffmpeg_args+=("-c:a" "alac" "$target")
      ;;
    wav|wave)
      ffmpeg_args+=("-c:a" "pcm_s16le" "$target")
      ;;
    aif|aiff|aifc)
      ffmpeg_args+=("-c:a" "pcm_s16be" "$target")
      ;;
    caf)
      ffmpeg_args+=("-c:a" "pcm_s16be" "$target")
      ;;
    flac)
      ffmpeg_args+=("-c:a" "flac" "$target")
      ;;
    ogg|oga|vorbis)
      ffmpeg_args+=("-c:a" "libvorbis" "$target")
      ;;
    opus|spx)
      ffmpeg_args+=("-c:a" "libopus" "$target")
      ;;
    ape)
      ffmpeg_args+=("-c:a" "ape" "$target")
      ;;
    tak)
      ffmpeg_args+=("-c:a" "tak" "$target")
      ;;
    wv)
      ffmpeg_args+=("-c:a" "wavpack" "$target")
      ;;
    mka|mkv|webm)
      ffmpeg_args+=("-c:a" "libopus" "$target")
      ;;
    ac3|ec3)
      ffmpeg_args+=("-c:a" "ac3" "$target")
      ;;
    dts)
      ffmpeg_args+=("-c:a" "dca" "$target")
      ;;
    amr|awb)
      ffmpeg_args+=("-c:a" "amr_nb" "$target")
      ;;
    mpc|mpc8)
      ffmpeg_args+=("-c:a" "musepack7" "$target")
      ;;
    *)
      return 1
      ;;
  esac

  echo "Generando $target"
  if ! ffmpeg "${ffmpeg_args[@]}" >/dev/null 2>&1; then
    echo "Aviso: no se pudo generar $target, se omite" >&2
    rm -f "$target"
  fi
}

extensions=(
  mp3 mpa m1a m2a aac adts m4a m4b m4p m4r mp4 3gp 3g2 mov qt wav wave
  aif aiff aifc caf flac ogg oga opus spx vorbis ape tak wv mka mkv webm
  ac3 ec3 dts amr awb mpc mpc8 alac
)

for ext in "${extensions[@]}"; do
  encode "$ext"
done

echo "Listo: $output_dir"
