using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using LuxiFer.App.Controls;
using LuxiFer.App.ViewModels;

namespace LuxiFer.App.Views;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        Canvas.PointerMillimeterMoved += (_, mm) => ViewModel?.ReportCursor(mm.X, mm.Y);
        Canvas.DocumentChanged += (_, _) => ViewModel?.MarkDirty();

        DataContextChanged += (_, _) =>
        {
            if (ViewModel is { } vm)
                vm.CanvasInvalidateRequested += (_, _) => Canvas.InvalidateVisual();
        };

        // Bett beim Einpassen zwischen den schwebenden Panelen freihalten
        // (links Werkzeug-Palette ~64px, rechts Layer-Panel ~294px).
        Canvas.ContentInset = new Thickness(72, 24, 300, 24);

        KeyDown += OnWindowKeyDown;
    }

    private MainWindowViewModel? ViewModel => DataContext as MainWindowViewModel;

    private void OnWindowKeyDown(object? sender, KeyEventArgs e)
    {
        if (ViewModel is null) return;

        // Undo/Redo mit Strg (auch bei fokussiertem Textfeld erlaubt)
        if (e.KeyModifiers == KeyModifiers.Control)
        {
            switch (e.Key)
            {
                case Key.Z:
                    ViewModel.UndoActionCommand.Execute(null);
                    e.Handled = true;
                    return;
                case Key.Y:
                    ViewModel.RedoActionCommand.Execute(null);
                    e.Handled = true;
                    return;
            }
            return;
        }

        // Werkzeug-Kürzel nicht auslösen, während in einem Textfeld getippt wird
        if (e.KeyModifiers != KeyModifiers.None) return;
        if (FocusManager?.GetFocusedElement() is TextBox) return;

        var tool = e.Key switch
        {
            Key.V => CanvasTool.Select,
            Key.R => CanvasTool.Rectangle,
            Key.E => CanvasTool.Ellipse,
            Key.L => CanvasTool.Line,
            Key.P => CanvasTool.Polyline,
            Key.G => CanvasTool.Polygon,
            _ => (CanvasTool?)null,
        };
        if (tool is { } t)
        {
            ViewModel.SelectToolCommand.Execute(t);
            e.Handled = true;
        }
    }

    private void OnLayerVisibilityClick(object? sender, RoutedEventArgs e) =>
        Canvas.InvalidateVisual();

    private void OnSelectionEditCommit(object? sender, RoutedEventArgs e) =>
        ViewModel?.CommitSelectionEdit();

    private void OnZoomToFitClick(object? sender, RoutedEventArgs e) =>
        Canvas.ZoomToFit();

    private void OnExitClick(object? sender, RoutedEventArgs e) => Close();
}
