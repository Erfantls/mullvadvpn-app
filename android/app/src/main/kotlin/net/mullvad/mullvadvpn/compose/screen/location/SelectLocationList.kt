package net.mullvad.mullvadvpn.compose.screen.location

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.intl.Locale
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.toLowerCase
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.tooling.preview.PreviewParameter
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.mullvad.mullvadvpn.R
import net.mullvad.mullvadvpn.compose.button.PrimaryButton
import net.mullvad.mullvadvpn.compose.component.MullvadCircularProgressIndicatorLarge
import net.mullvad.mullvadvpn.compose.component.drawVerticalScrollbar
import net.mullvad.mullvadvpn.compose.constant.ContentType
import net.mullvad.mullvadvpn.compose.extensions.animateScrollAndCentralizeItem
import net.mullvad.mullvadvpn.compose.preview.SearchLocationsListUiStatePreviewParameterProvider
import net.mullvad.mullvadvpn.compose.state.RelayListType
import net.mullvad.mullvadvpn.compose.state.SelectLocationListUiState
import net.mullvad.mullvadvpn.compose.util.RunOnKeyChange
import net.mullvad.mullvadvpn.lib.model.CustomListId
import net.mullvad.mullvadvpn.lib.model.Hop
import net.mullvad.mullvadvpn.lib.model.RelayItemId
import net.mullvad.mullvadvpn.lib.theme.AppTheme
import net.mullvad.mullvadvpn.lib.theme.Dimens
import net.mullvad.mullvadvpn.lib.theme.color.AlphaScrollbar
import net.mullvad.mullvadvpn.lib.ui.component.relaylist.RelayListItem
import net.mullvad.mullvadvpn.util.Lce
import net.mullvad.mullvadvpn.viewmodel.location.SelectLocationListViewModel
import org.koin.androidx.compose.koinViewModel
import org.koin.core.parameter.parametersOf

@Preview("Content|Loading|Error")
@Composable
private fun PreviewSelectLocationList(
    @PreviewParameter(SearchLocationsListUiStatePreviewParameterProvider::class)
    state: Lce<Unit, SelectLocationListUiState, Unit>
) {
    AppTheme {
        Surface {
            SelectLocationListContent(
                state = state,
                lazyListState = rememberLazyListState(),
                openDaitaSettings = {},
                onSelectHop = {},
                onUpdateBottomSheetState = {},
                onAddCustomList = {},
                onEditCustomLists = {},
                onToggleExpand = { id: RelayItemId, id1: CustomListId?, bool: Boolean -> },
            )
        }
    }
}

private typealias EntryBlocked = Lce.Error<Unit>

private typealias Content = Lce.Content<SelectLocationListUiState>

@Composable
fun SelectLocationList(
    relayListType: RelayListType,
    onSelectHop: (Hop) -> Unit,
    openDaitaSettings: () -> Unit,
    onAddCustomList: () -> Unit,
    onEditCustomLists: (() -> Unit)?,
    onUpdateBottomSheetState: (LocationBottomSheetState) -> Unit,
) {
    val viewModel =
        koinViewModel<SelectLocationListViewModel>(
            key = relayListType.name,
            parameters = { parametersOf(relayListType) },
        )
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    val stateActual = state

    val lazyListState = rememberLazyListState()
    RunOnKeyChange(stateActual is Content) {
        stateActual.indexOfSelectedRelayItem()?.let { index ->
            lazyListState.scrollToItem(index)
            lazyListState.animateScrollAndCentralizeItem(index)
        }
    }

    SelectLocationListContent(
        state = state,
        lazyListState = lazyListState,
        openDaitaSettings = openDaitaSettings,
        onSelectHop = onSelectHop,
        onUpdateBottomSheetState = onUpdateBottomSheetState,
        onAddCustomList = onAddCustomList,
        onEditCustomLists = onEditCustomLists,
        onToggleExpand = viewModel::onToggleExpand,
    )
}

@Composable
private fun SelectLocationListContent(
    state: Lce<Unit, SelectLocationListUiState, Unit>,
    lazyListState: LazyListState,
    openDaitaSettings: () -> Unit,
    onSelectHop: (Hop) -> Unit,
    onUpdateBottomSheetState: (LocationBottomSheetState) -> Unit,
    onAddCustomList: () -> Unit,
    onEditCustomLists: (() -> Unit)?,
    onToggleExpand: (RelayItemId, CustomListId?, Boolean) -> Unit,
) {
    var prevTopItem by remember { mutableStateOf<RelayListItem?>(null) }

    LazyColumn(
        modifier =
            Modifier.fillMaxSize()
                .padding(horizontal = Dimens.mediumPadding)
                .drawVerticalScrollbar(
                    lazyListState,
                    MaterialTheme.colorScheme.onSurface.copy(alpha = AlphaScrollbar),
                ),
        state = lazyListState,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement =
            if (state is EntryBlocked) {
                Arrangement.Center
            } else {
                Arrangement.Top
            },
    ) {
        when (state) {
            is Lce.Loading -> loading()
            is EntryBlocked -> entryBlocked(openDaitaSettings = openDaitaSettings)
            is Content -> {
                // When recents have been disabled and are enabled again and we are at the
                // top of the list we scroll up so that recents are visible again.
                val shouldScrollToTop =
                    state.value.relayListItems[0] is RelayListItem.RecentsListHeader &&
                        prevTopItem !is RelayListItem.RecentsListHeader &&
                        lazyListState.firstVisibleItemIndex == 0 &&
                        lazyListState.firstVisibleItemScrollOffset == 0

                prevTopItem = state.value.relayListItems[0]

                relayListContent(
                    relayListItems = state.value.relayListItems,
                    customLists = state.value.customLists,
                    onSelectHop = onSelectHop,
                    onToggleExpand = onToggleExpand,
                    onUpdateBottomSheetState = onUpdateBottomSheetState,
                    customListHeader = {
                        CustomListHeader(
                            onAddCustomList,
                            if (state.value.customLists.isNotEmpty()) onEditCustomLists else null,
                        )
                    },
                )

                if (shouldScrollToTop) {
                    lazyListState.requestScrollToItem(0)
                }
            }
        }
    }
}

private fun LazyListScope.loading() {
    item(contentType = ContentType.PROGRESS) { MullvadCircularProgressIndicatorLarge() }
}

private fun LazyListScope.entryBlocked(openDaitaSettings: () -> Unit) {
    item(contentType = ContentType.DESCRIPTION) {
        Text(
            text =
                stringResource(
                    R.string.multihop_entry_disabled_description,
                    stringResource(R.string.multihop).toLowerCase(Locale.current),
                    stringResource(id = R.string.daita),
                    stringResource(R.string.direct_only),
                ),
            style = MaterialTheme.typography.labelLarge,
            textAlign = TextAlign.Center,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = Dimens.mediumPadding),
        )
    }
    item(contentType = ContentType.SPACER) {
        Spacer(modifier = Modifier.height(Dimens.mediumPadding))
    }
    item(contentType = ContentType.BUTTON) {
        PrimaryButton(
            text =
                stringResource(R.string.open_feature_settings, stringResource(id = R.string.daita)),
            onClick = openDaitaSettings,
            modifier = Modifier.padding(horizontal = Dimens.mediumPadding),
        )
    }
}

private fun Lce<Unit, SelectLocationListUiState, Unit>.indexOfSelectedRelayItem(): Int? =
    if (this is Content) {
        val index =
            value.relayListItems.indexOfFirst {
                when (it) {
                    is RelayListItem.CustomListItem -> it.isSelected
                    is RelayListItem.GeoLocationItem -> it.isSelected
                    is RelayListItem.RecentListItem -> it.isSelected
                    is RelayListItem.CustomListEntryItem,
                    is RelayListItem.CustomListFooter,
                    RelayListItem.CustomListHeader,
                    RelayListItem.LocationHeader,
                    is RelayListItem.LocationsEmptyText,
                    is RelayListItem.EmptyRelayList,
                    RelayListItem.RecentsListFooter,
                    RelayListItem.RecentsListHeader,
                    is RelayListItem.SectionDivider -> false
                }
            }
        if (index >= 0) index else null
    } else {
        null
    }
