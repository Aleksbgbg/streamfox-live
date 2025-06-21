<script setup lang="ts">
import { type Ref, computed, onMounted, onUnmounted, ref } from "vue";
import { UserIcon } from "@heroicons/vue/24/solid";
import "media-chrome";
import { sort } from "@/arrays";
import { createSession, renegotiateSession, trickleIceCandidate } from "@/endpoints/room";
import { assertNotNull } from "@/errors";
import { computeSupportedVideoCodecs, videoCodecToString } from "@/rtc/codecs";
import { type Event, EventType, type StreamFailedError, StreamFailedErrorCode } from "@/rtc/event";
import { toHumanReadableList } from "@/strings";
import { config, dataChannelMessageToString } from "@/webrtc";

const pipAvailable = !!document.pictureInPictureEnabled;

const props = defineProps<{
  name: string;
}>();

enum Connection {
  Disconnected,
  Connecting,
  Connected,
  Error,
}

enum Channel {
  Closed,
  Open,
}

const connectionState = ref(Connection.Disconnected);
const channelState = ref(Channel.Closed);

const statusStyle = computed(() => {
  switch (connectionState.value) {
    case Connection.Error:
    case Connection.Disconnected:
      return "status-error";
    case Connection.Connecting:
      return "status-warning";
    case Connection.Connected:
      return "status-success";
    default:
      return undefined;
  }
});
const connectionActivity = computed(
  () =>
    connectionState.value === Connection.Disconnected ||
    connectionState.value === Connection.Connecting,
);

const peers = ref(0);
const users = computed(() => (channelState.value === Channel.Open ? peers.value + 1 : 0));

function closeChannel() {
  peers.value = 0;
  channelState.value = Channel.Closed;
}

interface Stream {
  isActive: boolean;
  active: ActiveStream | null;
  error: StreamFailedError | null;
}

interface ActiveStream {
  videoTransceiver: RTCRtpTransceiver;
  audioTransceiver: RTCRtpTransceiver;
  mediaStream: MediaStream;
}

const streams = new Map<string, Stream>();
const currentStreamId: Ref<string | null> = ref(null);
const currentStream = computed(() => {
  if (currentStreamId.value === null) {
    return null;
  }

  const stream = assertNotNull(streams.get(currentStreamId.value));

  if (!stream.isActive) {
    return null;
  }

  return assertNotNull(stream.active).mediaStream;
});

const loading = ref(false);
const streaming = computed(() => currentStreamId.value !== null);
const error = computed(() => {
  if (currentStreamId.value === null) {
    return null;
  }

  const stream = assertNotNull(streams.get(currentStreamId.value));

  if (stream.isActive) {
    return null;
  }

  return stream.error;
});

const streamCodec = computed(() => {
  if (error.value === null) {
    return null;
  }

  if (error.value.code !== StreamFailedErrorCode.UnsupportedVideoCodec) {
    return null;
  }

  return videoCodecToString(assertNotNull(error.value.unsupportedVideoCodecParams).source);
});
const supportedVideoCodecs = sort(computeSupportedVideoCodecs());
const configurationSupportedCodecs = toHumanReadableList(supportedVideoCodecs, videoCodecToString);

function videoLoaded() {
  loading.value = false;
}

let sessionConnection: RTCPeerConnection | null = null;

onMounted(async () => {
  const connection = (sessionConnection = new RTCPeerConnection(config));

  let sessionId: string | null = null;

  connection.addEventListener("negotiationneeded", async function () {
    const offer = await connection.createOffer();

    let answerSdp;
    if (sessionId === null) {
      const answer = await createSession(
        props.name,
        assertNotNull(offer.sdp),
        supportedVideoCodecs,
      );

      if (answer === null) {
        return;
      }

      sessionId = answer.sessionId;
      answerSdp = answer.sdp;
    } else {
      const answer = await renegotiateSession(props.name, sessionId, assertNotNull(offer.sdp));

      if (answer === null) {
        return;
      }

      answerSdp = answer.sdp;
    }

    await connection.setLocalDescription(offer);
    await connection.setRemoteDescription({ type: "answer", sdp: answerSdp });
  });
  connection.addEventListener("connectionstatechange", function () {
    switch (connection.connectionState) {
      case "new":
      case "connecting":
        connectionState.value = Connection.Connecting;
        break;
      case "connected":
        connectionState.value = Connection.Connected;
        break;
      case "disconnected":
      case "closed":
        closeChannel();
        connectionState.value = Connection.Disconnected;
        break;
      case "failed":
        closeChannel();
        connectionState.value = Connection.Error;
        break;
    }
  });
  connection.addEventListener("icecandidate", async function (event) {
    if (event.candidate === null) {
      return;
    }

    await trickleIceCandidate(props.name, assertNotNull(sessionId), event.candidate.toJSON());
  });

  const channel = connection.createDataChannel("main");
  channel.addEventListener("open", function () {
    channelState.value = Channel.Open;
  });
  channel.addEventListener("close", closeChannel);
  channel.addEventListener("message", async function (event) {
    const message: Event = JSON.parse(await dataChannelMessageToString(event.data));

    switch (message.type) {
      case EventType.UserJoined:
        ++peers.value;
        break;
      case EventType.UserLeft:
        --peers.value;
        break;
      case EventType.StreamStarted:
        {
          const streamId = message.streamStartedPayload.streamId;

          const videoTransceiver = connection.addTransceiver("video", { direction: "recvonly" });
          const audioTransceiver = connection.addTransceiver("audio", { direction: "recvonly" });
          const mediaStream = new MediaStream();

          mediaStream.addTrack(videoTransceiver.receiver.track);
          mediaStream.addTrack(audioTransceiver.receiver.track);

          streams.set(streamId, {
            isActive: true,
            active: {
              audioTransceiver,
              videoTransceiver,
              mediaStream,
            },
            error: null,
          });

          currentStreamId.value = streamId;
          loading.value = true;
        }
        break;
      case EventType.StreamFailed:
        {
          const streamId = message.streamFailedPayload.streamId;
          const err = message.streamFailedPayload.error;

          streams.set(streamId, {
            isActive: false,
            active: null,
            error: err,
          });

          currentStreamId.value = streamId;
          loading.value = false;
        }
        break;
      case EventType.StreamEnded:
        {
          const streamId = message.streamEndedPayload.streamId;

          const endedStream = assertNotNull(streams.get(streamId));
          streams.delete(streamId);

          if (currentStreamId.value === streamId) {
            const next = streams.entries().next();

            if (next.value === undefined) {
              currentStreamId.value = null;
            } else {
              const [streamId, nextStream] = next.value;

              currentStreamId.value = streamId;
              loading.value = nextStream.isActive;
            }
          }

          if (endedStream.isActive) {
            const stream = assertNotNull(endedStream.active);
            stream.audioTransceiver.stop();
            stream.videoTransceiver.stop();
          }
        }
        break;
    }
  });
});

onUnmounted(() => {
  if (sessionConnection === null) {
    return;
  }

  sessionConnection.close();
});
</script>

<template>
  <div class="flex h-full flex-col py-3">
    <h1 class="text-center text-2xl font-bold">{{ name }}</h1>
    <div class="flex min-h-0 min-w-0 grow items-center justify-center pt-2 pb-5">
      <div v-show="streaming" class="contents">
        <span v-show="loading" class="loading loading-infinity text-primary h-16 w-16"></span>
        <div
          v-if="!loading && error"
          role="alert"
          class="alert alert-error alert-soft mx-5 text-lg">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-6 w-6 shrink-0 stroke-current"
            fill="none"
            viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span v-if="error.code === StreamFailedErrorCode.UnsupportedVideoCodec"
            >Your device hardware or browser software do not support the video codec which the user
            has chosen for their video stream ({{ streamCodec }}). Your configuration only supports
            {{ configurationSupportedCodecs }}.</span
          >
        </div>
        <media-controller
          v-show="!loading && !error"
          class="h-full w-full bg-transparent"
          gesturesdisabled
          hotkeys="nospace nok">
          <video
            slot="media"
            class="h-full w-full"
            autoplay
            :srcObject="currentStream"
            @canplay="videoLoaded"
            @contextmenu="(e) => e.preventDefault()" />
          <media-control-bar>
            <div class="grow bg-[rgb(20_20_30_/_0.7)]" />
            <media-mute-button></media-mute-button>
            <media-volume-range></media-volume-range>
            <media-pip-button v-if="pipAvailable"></media-pip-button>
            <media-fullscreen-button></media-fullscreen-button>
          </media-control-bar>
        </media-controller>
      </div>
      <p v-show="!streaming" class="text-center text-2xl">no active stream</p>
    </div>
    <div class="flex">
      <div class="mx-auto">
        <div class="indicator">
          <div class="indicator-item indicator-start inline-grid *:[grid-area:1/1]">
            <div
              class="status status-lg animate-ping"
              :class="statusStyle"
              v-show="connectionActivity"></div>
            <div class="status status-lg" :class="statusStyle"></div>
          </div>
          <span
            class="indicator-item badge"
            :class="channelState === Channel.Open ? 'badge-secondary' : 'badge-warning'"
            >{{ users }}</span
          >
          <button class="btn btn-neutral btn-square">
            <UserIcon class="size-6" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
