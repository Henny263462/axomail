<script setup lang="ts">
import { computed } from "vue";

import { ICONS, type IconName, type IconShape } from "./icons";

const props = withDefaults(
  defineProps<{
    name: IconName;
    size?: number;
    strokeWidth?: number;
  }>(),
  { size: 16, strokeWidth: 1.75 },
);

const shapes = computed<readonly IconShape[]>(() => ICONS[props.name]);
</script>

<template>
  <svg
    class="icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="strokeWidth"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <template
      v-for="(shape, index) in shapes"
      :key="index"
    >
      <path
        v-if="'d' in shape"
        :d="shape.d"
        :fill="shape.fill ? 'currentColor' : 'none'"
      />
      <circle
        v-else
        :cx="shape.cx"
        :cy="shape.cy"
        :r="shape.r"
        :fill="shape.fill ? 'currentColor' : 'none'"
      />
    </template>
  </svg>
</template>

<style scoped>
.icon {
  display: inline-block;
  flex-shrink: 0;
  vertical-align: middle;
}
</style>
