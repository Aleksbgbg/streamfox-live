<script setup lang="ts">
import { ref } from "vue";
import { useRoute } from "vue-router";
import { useFetch } from "@/api";
import { silenceApiError } from "@/errors";
import { router } from "@/router";
import { generateRoomName } from "@/strings";

const route = useRoute();

function createRoomName(): string {
  if (route.query.name) {
    return route.query.name.toString();
  }

  return generateRoomName();
}

const name = ref(createRoomName());

const { execute, pending, error } = await useFetch({
  method: "post",
  url: "/room/validate-name",
  data: { name },
  immediate: false,
});

async function enter() {
  await execute();

  if (error.value) {
    silenceApiError(error.value);
    return;
  }

  router.push({ name: "room", params: { name: name.value } });
}
</script>

<template>
  <div class="card bg-base-300 card-md mx-auto mt-10 w-96 shadow-sm">
    <form class="card-body" @submit.prevent="enter">
      <h1 class="card-title">Hello, guest!</h1>
      <p>Join or create a room.</p>
      <input
        type="text"
        placeholder="room name"
        class="input w-full"
        :class="{ 'input-error': error }"
        v-model="name" />
      <div v-if="error" class="ml-1">
        <p class="text-error text-sm" v-for="err of error.specific.name" :key="err">{{ err }}</p>
      </div>
      <div class="card-actions">
        <button class="btn btn-primary w-full capitalize" :disabled="pending" type="submit">
          enter room
          <span v-show="pending" class="loading loading-spinner loading-xs" />
        </button>
      </div>
    </form>
  </div>
</template>
