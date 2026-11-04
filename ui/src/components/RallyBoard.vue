<script setup lang="ts">
import { computed } from 'vue';
import { HEIGHT, WIDTH, board } from '../lib/board';
import type { ViewRow } from '../api/types';

const props = defineProps<{ view: ViewRow }>();
const CELL = 24;
const drawn = computed(() => board(props.view));
</script>

<template>
  <figure class="rally" data-test="rally-board">
    <template v-if="drawn.ok">
      <figcaption>
        west needs {{ drawn.board.westNeeds }} · east needs {{ drawn.board.eastNeeds }}
        <strong v-if="drawn.board.winner" data-test="winner"> · {{ drawn.board.winner }} wins</strong>
      </figcaption>
      <svg :width="WIDTH * CELL + 2" :height="HEIGHT * CELL + 2" :viewBox="`-1 -1 ${WIDTH * CELL + 2} ${HEIGHT * CELL + 2}`">
        <rect x="0" y="0" :width="WIDTH * CELL" :height="HEIGHT * CELL" class="court" />
        <line :x1="(WIDTH * CELL) / 2" y1="0" :x2="(WIDTH * CELL) / 2" :y2="HEIGHT * CELL" class="net" />
        <rect data-test="west-paddle" :x="2" :y="drawn.board.west * CELL + 2" :width="CELL - 8" :height="CELL - 4" class="paddle" />
        <rect
          data-test="east-paddle"
          :x="(WIDTH - 1) * CELL + 6"
          :y="drawn.board.east * CELL + 2"
          :width="CELL - 8"
          :height="CELL - 4"
          class="paddle"
        />
        <circle
          data-test="ball"
          :cx="drawn.board.ball.x * CELL + CELL / 2"
          :cy="drawn.board.ball.y * CELL + CELL / 2"
          :r="CELL / 3"
          class="ball"
        />
      </svg>
    </template>
    <p v-else class="refused">{{ drawn.error }}</p>
  </figure>
</template>

<style scoped>
.rally {
  margin: 0;
}
.court {
  fill: var(--panel);
  stroke: var(--border);
}
.net {
  stroke: var(--border);
  stroke-dasharray: 4 4;
}
.paddle {
  fill: var(--fg);
}
.ball {
  fill: var(--accent);
}
.refused {
  color: var(--bad);
}
</style>
