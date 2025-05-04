import { type GenericError } from "@/api";

export function assertNotNull<T>(value: T | undefined | null): T {
  if (value === undefined || value === null) {
    throw new Error("precondition failed: value is null");
  }

  return value;
}

export function reportApiError(error: GenericError) {
  reportError(combine(error.generic, "API errors:"));
}

function reportError(error: Error | string) {
  console.error(error);
}

export function silenceApiError(error: GenericError) {
  silenceError(combine(error.generic, "Silent API errors:"));
}

function silenceError(error: Error | string) {
  if (!import.meta.env.DEV) {
    return;
  }

  console.error(error);
}

function combine(errors: string[], prefix: string = ""): string {
  let result = prefix;

  for (let index = 0; index < errors.length; ++index) {
    result += ` (${index + 1}) ${errors[index]}\n`;
  }

  return result;
}
