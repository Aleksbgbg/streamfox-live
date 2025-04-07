<script setup lang="ts">
import { computed, ref } from "vue";
import { UserIcon } from "@heroicons/vue/24/solid";

defineProps<{
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
const users = computed(() => peers.value + (channelState.value === Channel.Open ? 1 : 0));
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
