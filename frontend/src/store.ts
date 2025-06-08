const roomNameKey = "room-name";

export function retrieveRoomName(): string | null {
  return localStorage.getItem(roomNameKey);
}

export function storeRoomName(name: string) {
  localStorage.setItem(roomNameKey, name);
}

export function clearRoomName() {
  localStorage.removeItem(roomNameKey);
}
