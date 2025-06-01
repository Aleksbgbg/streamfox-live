export const config = {
  iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
};

export function videoCapabilitiesOrEmpty(): RTCRtpCapabilities {
  const capabilities = RTCRtpSender.getCapabilities("video");

  if (capabilities === null) {
    return {
      codecs: [],
      headerExtensions: [],
    };
  }

  return capabilities;
}

export async function dataChannelMessageToString(message: unknown): Promise<string> {
  if (message instanceof Blob) {
    return await message.text();
  } else if (message instanceof ArrayBuffer) {
    return new TextDecoder("utf-8").decode(message);
  } else {
    throw new Error("unexpected data channel message type");
  }
}
