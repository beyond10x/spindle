<script setup lang="ts">
import { computed } from 'vue';
import { counts } from './requests';
import type { RequestTotals } from '../api/types';

const props = defineProps<{ totals: RequestTotals }>();
const shown = computed(() => counts(props.totals));
</script>

<template>
  <p class="banner" data-test="banner">
    <span v-for="(count, index) in shown" :key="count.name" :class="count.name"
      >{{ index ? ' · ' : '' }}{{ count.label }} <b :data-test="`count-${count.name}`">{{ count.value }}</b></span
    >
  </p>
</template>
