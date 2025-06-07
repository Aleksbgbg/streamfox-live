export enum EventType {
  UserJoined,
  UserLeft,
  StreamStarted,
  StreamEnded,
}

interface StreamStartedPayload {
  streamId: string;
}

interface StreamEndedPayload {
  streamId: string;
}

export interface Event {
  type: EventType;
  streamStartedPayload: StreamStartedPayload;
  streamEndedPayload: StreamEndedPayload;
}
