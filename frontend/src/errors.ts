import { type GenericError } from "@/api";

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
