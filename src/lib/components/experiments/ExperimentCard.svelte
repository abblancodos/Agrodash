<script lang="ts">
  let { experiment }: { experiment: any } = $props();

  function statusColor(s: string) {
    return s === 'active' ? 'var(--live-color)' : 'var(--text-muted)';
  }
  function roleBg(r: string) {
    return r === 'admin' ? 'rgba(29,158,117,0.12)' : r === 'editor' ? 'rgba(56,138,221,0.12)' : 'var(--bg-elevated)';
  }
  function roleColor(r: string) {
    return r === 'admin' ? 'var(--live-color)' : r === 'editor' ? '#185FA5' : 'var(--text-muted)';
  }
</script>

<a href="/experiments/{experiment.id}" class="card">
  <div class="card__head">
    <span class="card__title">{experiment.title}</span>
    <span class="card__status" style="color:{statusColor(experiment.status)}">
      {experiment.status}
    </span>
  </div>
  {#if experiment.description}
    <p class="card__desc">{experiment.description}</p>
  {/if}
  <div class="card__foot">
    <div class="card__meta">
      {#if experiment.public}
        <span class="badge-pub">público</span>
      {/if}
      {#if experiment.user_role}
        <span class="badge-role"
              style="background:{roleBg(experiment.user_role)};color:{roleColor(experiment.user_role)}">
          {experiment.user_role}
        </span>
      {/if}
    </div>
    <span class="card__date">
      {new Date(experiment.created_at).toLocaleDateString('es-CR', {
        day: '2-digit', month: 'short', year: 'numeric'
      })}
    </span>
  </div>
</a>

<style>
  .card {
    display: flex; flex-direction: column; gap: calc(8px * var(--font-scale));
    padding: calc(14px * var(--font-scale)) calc(16px * var(--font-scale));
    background: var(--bg-surface);
    border: 0.5px solid var(--border-subtle);
    border-radius: 10px; text-decoration: none;
    transition: all .12s; cursor: pointer;
  }
  .card:hover { border-color: var(--border-default); background: var(--interactive-hover); }
  .card__head { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; }
  .card__title { font-size: calc(14px * var(--font-scale)); font-weight: 500; color: var(--text-primary); line-height: 1.3; }
  .card__status { font-size: calc(11px * var(--font-scale)); flex-shrink: 0; margin-top: 2px; }
  .card__desc { font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); line-height: 1.4; }
  .card__foot { display: flex; align-items: center; justify-content: space-between; margin-top: auto; }
  .card__meta { display: flex; align-items: center; gap: 6px; }
  .card__date { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); }
  .badge-pub { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 20px; background: var(--bg-elevated); color: var(--text-muted); }
  .badge-role { font-size: calc(10px * var(--font-scale)); padding: 2px 7px; border-radius: 20px; }
</style>
