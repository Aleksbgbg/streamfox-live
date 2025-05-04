<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
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

function closeChannel() {
  peers.value = 0;
  channelState.value = Channel.Closed;
}

enum EventType {
  UserJoined,
  UserLeft,
}

interface Event {
  type: EventType;
}

onMounted(async () => {
  const connection = new RTCPeerConnection(config);
  const channel = connection.createDataChannel("main");
  const offer = await connection.createOffer();

  const { data, error } = await useFetch<{ sessionId: string; sdp: string }>({
    method: "post",
    url: `/room/${props.name}/session`,
    data: {
      sdp: offer.sdp,
    },
    immediate: true,
  });

  if (error.value) {
    reportApiError(error.value);
    return;
  }

  const response = assertNotNull(data.value);

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
      url: `/room/${props.name}/session/${response.sessionId}`,
      data: {
        candidate: event.candidate.toJSON(),
      },
      immediate: true,
    });

    if (error.value) {
      reportApiError(error.value);
    }
  });

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
    }
  });

  await connection.setLocalDescription(offer);
  await connection.setRemoteDescription({ type: "answer", sdp: response.sdp });
});
</script>

<template>
  <div class="flex h-full flex-col py-3">
    <div>
      <h1 class="text-center text-2xl font-bold">{{ name }}</h1>
    </div>
    <div class="flex grow flex-col">
      <p class="my-auto text-center text-xl">no active stream</p>
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
