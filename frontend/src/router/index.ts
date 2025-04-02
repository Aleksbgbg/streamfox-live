import { createRouter, createWebHistory } from "vue-router";
import { useFetch } from "@/api";

export const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: "/",
      name: "main",
      component: () => import("@/views/main.vue"),
    },
    {
      path: "/room/:name",
      name: "room",
      component: () => import("@/views/room.vue"),
      props: true,
    },
  ],
});

router.beforeResolve(async function (to) {
  if (to.name === "room") {
    const { error } = await useFetch({
      method: "post",
      url: "/room/validate-name",
      data: { name: to.params.name },
      immediate: true,
    });

    if (error.value) {
      return { name: "main" };
    }
  }

  return true;
});
