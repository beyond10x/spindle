// @vitest-environment node
import { describe, expect, test } from 'vitest';
import { deploymentShape } from './shapes';
import { conform } from '../lib/conform';
import { raw } from '../../dev/observe-fixtures';
import type { Deployment } from './types';

describe('a periodic binding in the deployment answer', () => {
  test('the periodic binding of the deployment fixture has exactly the fields of Binding', () => {
    const deployment = raw('deployment.json') as Deployment;
    const periodic = deployment.bindings.filter((b) => b.kind === 'periodic');
    expect(periodic).toHaveLength(1);
    expect(conform({ ...deployment, bindings: periodic }, deploymentShape)).toEqual([]);
  });

  test('the shape refuses a schedule word that is not a string', () => {
    const deployment = raw('deployment.json') as Deployment;
    const tick = { ...deployment.bindings.find((b) => b.kind === 'periodic')!, every: 1 };
    expect(conform({ ...deployment, bindings: [tick] }, deploymentShape)).toEqual([
      '$.bindings[0].every: not a string',
    ]);
  });
});
