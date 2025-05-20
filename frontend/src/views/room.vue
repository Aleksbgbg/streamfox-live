<script setup lang="ts">
import { type Ref, computed, onMounted, ref } from "vue";
import { UserIcon } from "@heroicons/vue/24/solid";
import { useFetch } from "@/api";
import { assertNotNull, reportApiError } from "@/errors";
import { config, dataChannelMessageToString } from "@/webrtc";

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

const loading = ref(false);
const streaming = ref(false);

const video: Ref<HTMLVideoElement | null> = ref(null);

function closeChannel() {
  peers.value = 0;
  channelState.value = Channel.Closed;
}

enum EventType {
  UserJoined,
  UserLeft,
  StreamStarted,
  StreamEnded,
}

interface StreamStartedPayload {
  streamId: string;
}

interface StreamEndedPayload {
  streamId: string;
}

interface Event {
  type: EventType;
  streamStartedPayload: StreamStartedPayload;
  streamEndedPayload: StreamEndedPayload;
}

interface CreateSessionResponse {
  sessionId: string;
  sdp: string;
}

async function createSession(roomName: string, sdp: string): Promise<CreateSessionResponse | null> {
  const { data, error } = await useFetch<CreateSessionResponse>({
    method: "post",
    url: `/room/${roomName}/session`,
    data: {
      sdp,
    },
    immediate: true,
  });

  if (error.value) {
    reportApiError(error.value);
    return null;
  }

  return assertNotNull(data.value);
}

interface RenegotiateSessionResponse {
  sdp: string;
}

async function renegotiateSession(
  roomName: string,
  sessionId: string,
  sdp: string,
): Promise<RenegotiateSessionResponse | null> {
  const { data, error } = await useFetch<RenegotiateSessionResponse>({
    method: "post",
    url: `/room/${roomName}/session/${sessionId}`,
    data: {
      sdp,
    },
    immediate: true,
  });

  if (error.value) {
    reportApiError(error.value);
    return null;
  }

  return assertNotNull(data.value);
}

interface Stream {
  videoTransceiver: RTCRtpTransceiver;
  audioTransceiver: RTCRtpTransceiver;
  mediaStream: MediaStream;
}

onMounted(async () => {
  assertNotNull(video.value).addEventListener("canplay", () => (loading.value = false));

  const connection = new RTCPeerConnection(config);

  let sessionId: string | null = null;

  connection.addEventListener("negotiationneeded", async function () {
    const offer = await connection.createOffer();

    let answerSdp;
    if (sessionId === null) {
      const answer = await createSession(props.name, assertNotNull(offer.sdp));

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

    const { error } = await useFetch<{ sdp: string }>({
      method: "patch",
      url: `/room/${props.name}/session/${sessionId}`,
      data: {
        candidate: event.candidate.toJSON(),
      },
      immediate: true,
    });

    if (error.value) {
      reportApiError(error.value);
    }
  });

  const streams = new Map<string, Stream>();
  let currentStreamId: string | null = null;

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
            audioTransceiver,
            videoTransceiver,
            mediaStream,
          });

          assertNotNull(video.value).srcObject = mediaStream;
          currentStreamId = streamId;
          loading.value = true;
          streaming.value = true;
        }
        break;
      case EventType.StreamEnded:
        {
          const streamId = message.streamEndedPayload.streamId;

          if (currentStreamId === streamId) {
            streaming.value = false;
            currentStreamId = null;
            assertNotNull(video.value).srcObject = null;
          }

          const stream = assertNotNull(streams.get(streamId));
          streams.delete(streamId);

          stream.audioTransceiver.stop();
          stream.videoTransceiver.stop();
        }
        break;
    }
  });
});
</script>

<template>
  <div class="flex h-full flex-col py-3">
    <div>
      <h1 class="text-center text-2xl font-bold">{{ name }}</h1>
    </div>
    <div class="flex min-h-0 min-w-0 grow items-center justify-center pt-2 pb-5">
      <div v-show="streaming" class="contents">
        <span v-show="loading" class="loading loading-infinity text-primary h-16 w-16"></span>
        <video v-show="!loading" ref="video" class="max-h-full max-w-full" autoplay />
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
