<script setup lang="ts">
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
  <h1>{{ name }}</h1>
</template>
