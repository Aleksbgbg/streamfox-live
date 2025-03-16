<script setup lang="ts">
import { ref } from "vue";
import { generateRoomName } from "@/strings";

const name = ref(generateRoomName());
</script>

<template>
  <div class="card bg-base-300 card-md mx-auto mt-10 w-96 shadow-sm">
    <div class="card-body">
      <h1 class="card-title">Hello, guest!</h1>
      <p>Join or create a room.</p>
      <input type="text" placeholder="room name" class="input w-full" v-model="name" />
      <div class="card-actions">
        <button class="btn btn-primary w-full capitalize">enter room</button>
      </div>
    </div>
  </div>
</template>
