import { useFetch } from "@/api";
import { assertNotNull, reportApiError } from "@/errors";
import { VideoCodec } from "@/rtc/codecs";

interface CreateSessionResponse {
  sessionId: string;
  sdp: string;
}

export async function createSession(
  roomName: string,
  sdp: string,
  supportedVideoCodecs: VideoCodec[],
): Promise<CreateSessionResponse | null> {
  const { data, error } = await useFetch<CreateSessionResponse>({
    method: "post",
    url: `/room/${roomName}/session`,
    data: {
      sdp,
      supportedVideoCodecs,
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

export async function renegotiateSession(
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

export async function trickleIceCandidate(
  roomName: string,
  sessionId: string,
  candidate: RTCIceCandidateInit,
): Promise<void> {
  const { error } = await useFetch<void>({
    method: "patch",
    url: `/room/${roomName}/session/${sessionId}`,
    data: {
      candidate,
    },
    immediate: true,
  });

  if (error.value) {
    reportApiError(error.value);
  }
}
