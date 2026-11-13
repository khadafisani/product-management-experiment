<script lang="ts">
import {page} from '$app/state';
import DetailPage from '../../../lib/DetailPage.svelte';
</script>
<DetailPage category="document" id={page.params.id ?? ''} />
