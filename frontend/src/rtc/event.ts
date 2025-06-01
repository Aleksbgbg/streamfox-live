import { VideoCodec } from "@/rtc/codecs";

export enum EventType {
  UserJoined,
  UserLeft,
  StreamStarted,
  StreamFailed,
  StreamEnded,
}

interface StreamStartedPayload {
  streamId: string;
}

export enum StreamFailedErrorCode {
  UnsupportedVideoCodec,
}

interface UnsupportedVideoCodecParams {
  source: VideoCodec;
}

export interface StreamFailedError {
  code: StreamFailedErrorCode;
  unsupportedVideoCodecParams: UnsupportedVideoCodecParams | null;
}

interface StreamFailedPayload {
  streamId: string;
  error: StreamFailedError;
}

interface StreamEndedPayload {
  streamId: string;
}

export interface Event {
  type: EventType;
  streamStartedPayload: StreamStartedPayload;
  streamFailedPayload: StreamFailedPayload;
  streamEndedPayload: StreamEndedPayload;
}
