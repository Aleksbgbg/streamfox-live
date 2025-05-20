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
      beforeEnter: [
        function (to) {
          // When joining a room and there is an active video stream, we want
          // to autoplay the video to give the user a sense that the stream is
          // always on.
          //
          // However, for user experience reasons, modern browsers prevent
          // autoplay before the user has interacted with the page.
          //
          // In that case, redirect the user back to the main page, where they
          // can click the entry button, causing an interaction and allowing
          // the stream to autoplay.
          if (navigator.userActivation.hasBeenActive) {
            return;
          }

          router.push({ name: "main", query: { name: to.params.name } });
        },
      ],
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
