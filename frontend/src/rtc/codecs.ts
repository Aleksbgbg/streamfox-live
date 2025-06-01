import { videoCapabilitiesOrEmpty } from "@/webrtc";

export enum VideoCodec {
  H264,
  H265,
  VP8,
  VP9,
  AV1,
}

export function videoCodecToString(codec: VideoCodec): string {
  switch (codec) {
    case VideoCodec.H264:
      return "H264";
    case VideoCodec.H265:
      return "H265";
    case VideoCodec.VP8:
      return "VP8";
    case VideoCodec.VP9:
      return "VP9";
    case VideoCodec.AV1:
      return "AV1";
  }
}

function mimeTypeToVideoCodec(mimeType: string): VideoCodec | null {
  switch (mimeType) {
    case "video/H264":
      return VideoCodec.H264;
    case "video/H265":
      return VideoCodec.H265;
    case "video/VP8":
      return VideoCodec.VP8;
    case "video/VP9":
      return VideoCodec.VP9;
    case "video/AV1":
      return VideoCodec.AV1;
    default:
      return null;
  }
}

export function computeSupportedVideoCodecs(): VideoCodec[] {
  const codecs = new Set<VideoCodec>();

  for (const codec of videoCapabilitiesOrEmpty().codecs) {
    const videoCodec = mimeTypeToVideoCodec(codec.mimeType);

    if (videoCodec != null) {
      codecs.add(videoCodec);
    }
  }

  return Array.from(codecs);
}
