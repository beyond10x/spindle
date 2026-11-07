// The observe panel a served run answers at `GET /__spindle/ui`: one page, its script and style
// inlined by `vite.panel.config.ts`, reading `./requests` beside itself.

import { createApp } from 'vue';
import Panel from './Panel.vue';
import './panel.css';

createApp(Panel).mount('#panel');
