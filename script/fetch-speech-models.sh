#!/usr/bin/env bash
# Fetch the speech models the iPhone app carries, which are too big for git:
# SenseVoice Small (int8, as converted for sherpa-onnx, with its license) and
# the Silero voice activity detector, into fastxt-ios/SpeechModels/. Each
# download is checked against the SHA-256 that GitHub records for it, written
# here. Two of the sample recordings that come with SenseVoice go to
# fastxt-ios/FastxtTests/SpeechSamples/ for the unit tests.
#
# Without the models the app still builds, and dictation then uses only the
# recognizer built into iOS. Usage: script/fetch-speech-models.sh
set -euo pipefail
cd "$(dirname "$0")/../fastxt-ios"

release=https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models
sense_voice=sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17
sense_voice_sha256=7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e
vad=silero_vad.onnx
vad_sha256=9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6

stamp=.speech-models
want="$sense_voice $sense_voice_sha256 $vad $vad_sha256"
if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$want" ] &&
  [ -f SpeechModels/sense-voice/model.int8.onnx ] && [ -f SpeechModels/$vad ]; then
  echo "The speech models are already in fastxt-ios/SpeechModels."
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fetch() { # url sha256 file
  curl -fL --retry 3 --progress-bar -o "$3" "$1"
  if ! echo "$2  $3" | shasum -a 256 -c - >/dev/null; then
    echo "error: $1 isn't the file this script expects" >&2
    exit 1
  fi
}

fetch "$release/$sense_voice.tar.bz2" "$sense_voice_sha256" "$tmp/sense-voice.tar.bz2"
fetch "$release/$vad" "$vad_sha256" "$tmp/$vad"
tar -xjf "$tmp/sense-voice.tar.bz2" -C "$tmp"

rm -rf SpeechModels/sense-voice FastxtTests/SpeechSamples
mkdir -p SpeechModels/sense-voice FastxtTests/SpeechSamples
cp "$tmp/$sense_voice/model.int8.onnx" "$tmp/$sense_voice/tokens.txt" "$tmp/$sense_voice/LICENSE" \
  SpeechModels/sense-voice/
cp "$tmp/$vad" SpeechModels/
cp "$tmp/$sense_voice/test_wavs/en.wav" "$tmp/$sense_voice/test_wavs/zh.wav" FastxtTests/SpeechSamples/
echo "$want" >"$stamp"
echo "SenseVoice Small and the Silero VAD are in fastxt-ios/SpeechModels."
