import { isProxy, isReactive, isRef, ref, unref } from "vue";
import axios from "axios";

export interface GenericError {
  generic: string[];
  specific: Record<string, string[]>;
}

type KVMap = Record<string, unknown>;

const instance = axios.create();

export async function useFetch<Data, Error = GenericError>(params: {
  method: "get" | "post" | "put" | "patch" | "delete";
  url: string;
  data?: KVMap;
  immediate: boolean;
}) {
  const pending = ref(false);

  const data = ref<Data | undefined>(undefined);
  const error = ref<Error | undefined>(undefined);

  const clear = () => {
    data.value = undefined;
    error.value = undefined;
  };

  const execute = async () => {
    pending.value = true;

    await instance({
      method: params.method,
      url: "/api" + params.url,
      data: params.data ? deepToRaw(params.data) : undefined,
    })
      .then((response) => {
        clear();
        data.value = response.data;
      })
      .catch((err) => {
        clear();

        if (typeof err.response.data === "string") {
          error.value = {
            generic: [`Request failed with code ${err.status} (${err.code}).`],
            specific: {},
          };
        } else {
          error.value = err.response.data;
        }
      });

    pending.value = false;
  };

  if (params.immediate) {
    await execute();
  }

  return { execute, pending, data, error };
}

function deepToRaw(source: KVMap | KVMap[]): KVMap | KVMap[] {
  if (Array.isArray(source)) {
    return source.map(deepToRaw) as KVMap[];
  }

  if (isRef(source) || isReactive(source) || isProxy(source)) {
    return deepToRaw(unref(source));
  }

  if (typeof source === "object") {
    const result: KVMap = {};
    for (const key of Object.keys(source)) {
      result[key] = deepToRaw(source[key] as KVMap);
    }
    return result;
  }

  return source;
}
