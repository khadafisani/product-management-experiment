<script lang="ts">
import {page} from '$app/state';
import DetailPage from '../../../lib/DetailPage.svelte';
</script>
<DetailPage category="product" id={page.params.id ?? ''} />
