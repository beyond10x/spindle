// A runtime description of a TypeScript object type, and a check of a JSON value against it.
//
// `Shape<T>` requires one entry for every field of `T`, optional ones marked, so `vue-tsc` refuses
// a shape that is missing a field or declares one `T` does not have. `conform` then checks a value
// against the shape at run time: together they say a JSON answer has exactly the fields of `T`.

type Defined<T> = Exclude<T, undefined>;

export type Field<T> = [T] extends [string]
  ? string extends T
    ? { type: 'string' }
    : { type: 'string'; values: readonly T[] }
  : [T] extends [number]
    ? { type: 'number' }
    : [T] extends [boolean]
      ? { type: 'boolean' }
      : [T] extends [readonly (infer E)[]]
        ? { type: 'array'; of: Field<E> }
        : [T] extends [object]
          ? { type: 'object'; fields: Shape<T> }
          : never;

export type Shape<T> = {
  [K in keyof T]-?: Field<Defined<T[K]>> &
    (undefined extends T[K] ? { optional: true } : { optional?: never });
};

type AnyField =
  | { type: 'string'; values?: readonly string[]; optional?: boolean }
  | { type: 'number'; optional?: boolean }
  | { type: 'boolean'; optional?: boolean }
  | { type: 'array'; of: AnyField; optional?: boolean }
  | { type: 'object'; fields: Record<string, AnyField>; optional?: boolean };

/** Every way `value` differs from `shape`, each as `<path>: <reason>`; empty when it conforms. */
export function conform<T>(value: unknown, shape: Shape<T>): string[] {
  const problems: string[] = [];
  object(value, shape as unknown as Record<string, AnyField>, '$', problems);
  return problems;
}

function object(value: unknown, fields: Record<string, AnyField>, path: string, out: string[]) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    out.push(`${path}: not an object`);
    return;
  }
  const record = value as Record<string, unknown>;
  for (const [name, field] of Object.entries(fields)) {
    if (!(name in record)) {
      if (!field.optional) out.push(`${path}.${name}: missing`);
      continue;
    }
    check(record[name], field, `${path}.${name}`, out);
  }
  for (const name of Object.keys(record)) {
    if (!(name in fields)) out.push(`${path}.${name}: not declared`);
  }
}

function check(value: unknown, field: AnyField, path: string, out: string[]) {
  switch (field.type) {
    case 'string':
      if (typeof value !== 'string') out.push(`${path}: not a string`);
      else if (field.values && !field.values.includes(value)) {
        out.push(`${path}: ${JSON.stringify(value)} is not one of ${field.values.join(', ')}`);
      }
      return;
    case 'number':
      if (typeof value !== 'number' || !Number.isFinite(value)) out.push(`${path}: not a number`);
      return;
    case 'boolean':
      if (typeof value !== 'boolean') out.push(`${path}: not a boolean`);
      return;
    case 'array':
      if (!Array.isArray(value)) out.push(`${path}: not an array`);
      else value.forEach((item, index) => check(item, field.of, `${path}[${index}]`, out));
      return;
    case 'object':
      object(value, field.fields, path, out);
      return;
  }
}
