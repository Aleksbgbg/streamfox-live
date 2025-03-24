<script setup lang="ts">
import { UserIcon } from "@heroicons/vue/24/solid";
import { useFetch } from "@/api";
import { router } from "@/router";

const props = defineProps<{
  name: string;
}>();

const { error } = await useFetch({
  method: "post",
  url: "/room/validate-name",
  data: { name: props.name },
  immediate: true,
});

if (error.value) {
  router.push({ name: "main" });
}
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
          <span class="indicator-item badge badge-secondary">0</span>
          <button class="btn btn-neutral btn-square">
            <UserIcon class="size-6" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
