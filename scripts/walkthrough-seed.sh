#!/usr/bin/env bash
# Seed a local homeserver with the fixture content the screenshot walkthrough
# needs: a few unencrypted rooms with plausible conversations, a poll with
# responses, a thread, a voice message and a space with children.
#
# Rooms are deliberately UNENCRYPTED: the screenshots do not need E2EE and
# skipping it removes key-sharing/decryption timing from the walkthrough, which
# is what would otherwise make the seeded timelines flaky.
#
# Usage: scripts/walkthrough-seed.sh [homeserver-url] > /tmp/walkthrough.env
# Writes an env file on stdout for scripts/walkthrough.sh to source.
# Progress goes to stderr.
#
# Dependencies: curl, jq, python3 (for the voice-message WAV).
set -euo pipefail

HS=${1:-http://127.0.0.1:8008}
SUFFIX=${WALKTHROUGH_SEED_SUFFIX:-$(date +%s)}
PASSWORD=${WALKTHROUGH_SEED_PASSWORD:-walkthrough-demo-pw}

log() { printf '[seed] %s\n' "$*" >&2; }
api() { # api <token> <method> <path> [body]
    local token=$1 method=$2 path=$3 body=${4:-}
    local args=(-sS -X "$method" -H "Authorization: Bearer $token")
    [ -n "$body" ] && args+=(-H 'Content-Type: application/json' -d "$body")
    curl "${args[@]}" "$HS$path"
}
send() { # send <token> <room> <type> <content> -> event id
    local txn="w$RANDOM$RANDOM"
    api "$1" PUT "/_matrix/client/v3/rooms/$2/send/$3/$txn" "$4" | jq -r .event_id
}
state() { # state <token> <room> <type> <state-key> <content>
    api "$1" PUT "/_matrix/client/v3/rooms/$2/state/$3/$4" "$5" > /dev/null
}

register() { # register <localpart> <display name> -> token
    local user="$1$SUFFIX"
    local body
    body=$(jq -nc --arg u "$user" --arg p "$PASSWORD" \
        '{username:$u, password:$p, auth:{type:"m.login.dummy"}, inhibit_login:false}')
    local resp
    resp=$(curl -sS -X POST -H 'Content-Type: application/json' -d "$body" \
        "$HS/_matrix/client/v3/register")
    local token
    token=$(echo "$resp" | jq -r '.access_token // empty')
    [ -n "$token" ] || { log "registration of $user failed: $resp"; exit 1; }
    local uid
    uid=$(echo "$resp" | jq -r .user_id)
    api "$token" PUT "/_matrix/client/v3/profile/$uid/displayname" \
        "$(jq -nc --arg n "$2" '{displayname:$n}')" > /dev/null
    echo "$token $uid"
}

text() { jq -nc --arg b "$1" '{msgtype:"m.text", body:$b}'; }

log "registering demo users on $HS"
read -r DEMO_TOKEN DEMO_ID <<< "$(register demo 'Ada Lovelace')"
read -r BOB_TOKEN BOB_ID <<< "$(register bob 'Grace Hopper')"
read -r CAROL_TOKEN CAROL_ID <<< "$(register carol 'Katherine Johnson')"
log "demo user is $DEMO_ID"

create_room() { # create_room <name> <topic> -> room id
    local body
    body=$(jq -nc --arg n "$1" --arg t "$2" --arg b "$BOB_ID" --arg c "$CAROL_ID" \
        '{name:$n, topic:$t, preset:"public_chat", visibility:"public",
          invite:[$b,$c]}')
    local id
    id=$(api "$DEMO_TOKEN" POST /_matrix/client/v3/createRoom "$body" | jq -r .room_id)
    api "$BOB_TOKEN" POST "/_matrix/client/v3/join/$id" '{}' > /dev/null
    api "$CAROL_TOKEN" POST "/_matrix/client/v3/join/$id" '{}' > /dev/null
    echo "$id"
}

log "creating rooms"
MAIN=$(create_room 'Mandelbrot Design' 'Shipping the GNOME Matrix client')
ROOM2=$(create_room 'Release Engineering' 'Flatpak builds, tags and changelogs')
ROOM3=$(create_room 'Watercooler' 'Off-topic, tea and biscuits')

log "seeding conversation in $MAIN"
send "$BOB_TOKEN" "$MAIN" m.room.message "$(text 'Morning! The new call view landed on main last night.')" > /dev/null
send "$DEMO_TOKEN" "$MAIN" m.room.message "$(text 'Nice. Did the participant grid survive the resize work?')" > /dev/null
send "$CAROL_TOKEN" "$MAIN" m.room.message "$(text 'It did — it reflows down to three columns on a narrow window now.')" > /dev/null
send "$BOB_TOKEN" "$MAIN" m.room.message "$(text 'I also finished the voice message recorder, waveform and all.')" > /dev/null

log "seeding a thread"
THREAD_ROOT=$(send "$DEMO_TOKEN" "$MAIN" m.room.message "$(text 'What should we cut for the 15.0 release?')")
LAST=$THREAD_ROOT
for reply in \
    'Threads and polls are both in good shape, I would ship those.' \
    'Agreed. Spaces need one more pass on the overview page.' \
    'Voice messages too — recording works, playback scrubbing is rough.' \
    'Then it is threads, polls and voice for 15.0, spaces for 15.1.'; do
    body=$(jq -nc --arg b "$reply" --arg root "$THREAD_ROOT" --arg last "$LAST" \
        '{msgtype:"m.text", body:$b,
          "m.relates_to":{rel_type:"m.thread", event_id:$root,
                          is_falling_back:true,
                          "m.in_reply_to":{event_id:$last}}}')
    case $LAST in
        "$THREAD_ROOT") TOKEN=$BOB_TOKEN ;;
        *) TOKEN=$CAROL_TOKEN ;;
    esac
    LAST=$(send "$TOKEN" "$MAIN" m.room.message "$body")
done

log "seeding a poll"
POLL=$(send "$DEMO_TOKEN" "$MAIN" org.matrix.msc3381.poll.start '{
  "org.matrix.msc3381.poll.start": {
    "question": {"org.matrix.msc1767.text": "When should we cut 15.0?"},
    "kind": "org.matrix.msc3381.poll.disclosed",
    "max_selections": 1,
    "answers": [
      {"id": "friday", "org.matrix.msc1767.text": "This Friday"},
      {"id": "next-week", "org.matrix.msc1767.text": "Next week"},
      {"id": "after-gnome", "org.matrix.msc1767.text": "After the GNOME release"}
    ]
  },
  "org.matrix.msc1767.text": "When should we cut 15.0?"
}')
for pair in "$BOB_TOKEN:next-week" "$CAROL_TOKEN:next-week" "$DEMO_TOKEN:friday"; do
    tok=${pair%%:*}; answer=${pair##*:}
    body=$(jq -nc --arg p "$POLL" --arg a "$answer" \
        '{"org.matrix.msc3381.poll.response":{answers:[$a]},
          "m.relates_to":{rel_type:"m.reference", event_id:$p}}')
    send "$tok" "$MAIN" org.matrix.msc3381.poll.response "$body" > /dev/null
done

log "seeding a voice message"
WAV=$(mktemp --suffix=.wav)
python3 - "$WAV" <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(8000)
    w.writeframes(b"".join(
        struct.pack("<h", int(9000 * math.sin(i / 12.0) * math.exp(-((i % 8000) / 6000.0))))
        for i in range(8000 * 4)))
PY
MXC=$(curl -sS -X POST -H "Authorization: Bearer $BOB_TOKEN" \
    -H 'Content-Type: audio/wav' --data-binary "@$WAV" \
    "$HS/_matrix/media/v3/upload?filename=voice-message.wav" | jq -r .content_uri)
rm -f "$WAV"
if [ -n "$MXC" ] && [ "$MXC" != null ]; then
    WAVEFORM=$(python3 -c 'import math;print([int(512+511*math.sin(i/3.0)*math.cos(i/11.0)) for i in range(60)])' | tr -d ' ')
    body=$(jq -nc --arg u "$MXC" --argjson wf "$WAVEFORM" \
        '{msgtype:"m.audio", body:"Voice message",
          url:$u,
          info:{mimetype:"audio/wav", duration:4000, size:64044},
          "org.matrix.msc3245.voice":{},
          "org.matrix.msc1767.audio":{duration:4000, waveform:$wf}}')
    send "$BOB_TOKEN" "$MAIN" m.room.message "$body" > /dev/null
else
    log "WARNING: media upload failed, no voice message seeded"
fi

send "$CAROL_TOKEN" "$MAIN" m.room.message "$(text 'Playback sounds great on my machine 🎧')" > /dev/null

log "seeding the other rooms"
send "$BOB_TOKEN" "$ROOM2" m.room.message "$(text 'Nightly flatpak build is green again.')" > /dev/null
send "$DEMO_TOKEN" "$ROOM2" m.room.message "$(text 'Tagging 14.1 this afternoon then.')" > /dev/null
send "$CAROL_TOKEN" "$ROOM3" m.room.message "$(text 'Anyone else stuck on the crossword?')" > /dev/null

log "creating a space"
SPACE=$(api "$DEMO_TOKEN" POST /_matrix/client/v3/createRoom "$(jq -nc \
    '{name:"TunaOS", topic:"Everything Mandelbrot",
      preset:"public_chat", visibility:"public",
      creation_content:{type:"m.space"}}')" | jq -r .room_id)
# `via` must be the homeserver's own name, not the URL it is reached at.
SERVER_NAME=${DEMO_ID#*:}
for child in "$MAIN" "$ROOM2" "$ROOM3"; do
    state "$DEMO_TOKEN" "$SPACE" m.space.child "$child" \
        "$(jq -nc --arg v "$SERVER_NAME" '{via:[$v]}')"
done

log "done"
cat <<EOF
MANDELBROT_WALKTHROUGH_HOMESERVER=$HS
MANDELBROT_WALKTHROUGH_USER=demo$SUFFIX
MANDELBROT_WALKTHROUGH_PASSWORD=$PASSWORD
MANDELBROT_WALKTHROUGH_ROOM=$MAIN
MANDELBROT_WALKTHROUGH_SPACE=$SPACE
MANDELBROT_WALKTHROUGH_THREAD_ROOT=$THREAD_ROOT
MANDELBROT_WALKTHROUGH_ROOMS=4
EOF
