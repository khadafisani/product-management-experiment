<script lang="ts">
	import { tick } from 'svelte';
	import Icon from './Icon.svelte';
	export type RowAction = {
		label: string;
		icon: string;
		danger?: boolean;
		run?: () => unknown;
		href?: string;
	};
	let {
		href,
		label,
		items = [],
		disabled = false,
		primaryIcon = 'eye',
		primaryLabel = '',
		primaryTitle = 'View details'
	}: {
		href?: string;
		label: string;
		items?: RowAction[];
		disabled?: boolean;
		primaryIcon?: string;
		primaryLabel?: string;
		primaryTitle?: string;
	} = $props();
	let trigger = $state<HTMLButtonElement>();
	let popup: HTMLDivElement;
	let open = $state(false),
		top = $state(0),
		left = $state(0);
	const popupId = $props.id();
	function close(restoreFocus = false) {
		if (popup?.matches(':popover-open')) popup.hidePopover();
		open = false;
		if (restoreFocus) trigger?.focus();
	}
	async function toggle() {
		if (open) {
			close(true);
			return;
		}
		if (!trigger) return;
		const rect = trigger.getBoundingClientRect();
		left = Math.max(8, Math.min(rect.right - 228, window.innerWidth - 236));
		top = rect.bottom + 6;
		popup.showPopover();
		open = true;
		await tick();
		const height = popup.getBoundingClientRect().height;
		if (top + height > window.innerHeight - 8) top = Math.max(8, rect.top - height - 6);
		popup.querySelector<HTMLElement>('[role=menuitem]')?.focus();
	}
	function keyboard(event: KeyboardEvent) {
		const buttons = Array.from(
			popup.querySelectorAll<HTMLElement>('[role=menuitem]:not(:disabled)')
		);
		const index = buttons.indexOf(document.activeElement as HTMLElement);
		if (event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			close(true);
		} else if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
			event.preventDefault();
			const next =
				event.key === 'Home'
					? 0
					: event.key === 'End'
						? buttons.length - 1
						: (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
			buttons[next]?.focus();
		} else if (event.key === 'Tab') close();
	}
	function choose(item: RowAction) {
		close(true);
		void item.run?.();
	}
</script>

<svelte:window onscroll={() => close()} onresize={() => close()} />
<div class="row-actions">
	<!-- The primary action stays visible; secondary actions retain text in the menu. -->
	{#if href}<a
			class="row-icon"
			{href}
			aria-label={primaryLabel || `View ${label}`}
			title={primaryTitle}><Icon name={primaryIcon} size={16} /></a
		>{/if}
	{#if items.length}<button
			class="row-icon"
			bind:this={trigger}
			type="button"
			aria-label={`More actions for ${label}`}
			title="More actions"
			aria-haspopup="menu"
			aria-expanded={open}
			aria-controls={popupId}
			{disabled}
			onclick={toggle}><Icon name="more" size={19} /></button
		>{/if}
</div>
<div
	bind:this={popup}
	id={popupId}
	popover="auto"
	role="menu"
	tabindex="-1"
	aria-label={`Actions for ${label}`}
	class="row-action-menu"
	style={`top:${top}px;left:${left}px`}
	ontoggle={(event) => (open = event.newState === 'open')}
	onkeydown={keyboard}
>
	{#each items as item}
		{#if item.href}<a
				role="menuitem"
				href={item.href}
				class:danger={item.danger}
				onclick={() => close()}><Icon name={item.icon} size={16} /><span>{item.label}</span></a
			>
		{:else}<button
				type="button"
				role="menuitem"
				class:danger={item.danger}
				{disabled}
				onclick={() => choose(item)}
				><Icon name={item.icon} size={16} /><span>{item.label}</span></button
			>{/if}
	{/each}
</div>

<style>
	.row-actions {
		display: flex;
		gap: 6px;
		align-items: center;
		flex-wrap: nowrap;
	}
	.row-actions .row-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		padding: 0;
		margin: 0;
		border: 1px solid #e0e8e3;
		border-radius: 7px;
		background: white;
		color: #62776a;
		flex-shrink: 0;
	}
	.row-icon:hover {
		background: #edf5ef;
		color: #1c7755;
		border-color: #bed8ca;
	}
	.row-icon:focus-visible {
		outline: 2px solid #309473;
		outline-offset: 2px;
	}
	.row-action-menu {
		position: fixed;
		inset: auto;
		margin: 0;
		width: 228px;
		padding: 6px;
		background: white;
		color: #2a4034;
		border: 1px solid #dfe7e2;
		border-radius: 10px;
		box-shadow: 0 10px 32px #20392a26;
	}
	.row-action-menu :is(button, a) {
		margin: 0;
		border: 0;
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		text-align: left;
		padding: 11px 12px;
		border-radius: 6px;
		font-size: 13px;
		line-height: 20px;
		background: white;
	}
	.row-action-menu :is(button, a):hover,
	.row-action-menu :is(button, a):focus-visible {
		background: #edf5ef;
		outline: none;
	}
	.row-action-menu :is(button, a).danger {
		color: #b54e40;
	}
	.row-action-menu :is(button, a).danger:hover,
	.row-action-menu :is(button, a).danger:focus-visible {
		background: #fff0eb;
	}
	@media (pointer: coarse) {
		.row-actions .row-icon {
			width: 42px;
			height: 42px;
		}
	}
</style>
